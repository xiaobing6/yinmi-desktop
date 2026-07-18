use std::sync::Arc;

use super::{
    IsolationCounters, ResourcePolicyMetadata, ResourceRequestDecision,
    classify_resource_request_for,
};
use crate::feasibility::signature_webview::SignatureError;
use url::Url;
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        ICoreWebView2, ICoreWebView2_22,
    },
    WebResourceRequestedEventHandler,
};
use windows_core::{HSTRING, Interface, PWSTR};
use wry::WebViewExtWindows;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsFilterMode {
    AllSourceKinds,
    LegacyAllContextsCandidate,
}

impl WindowsFilterMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AllSourceKinds => "webview2-22-all-source-kinds",
            Self::LegacyAllContextsCandidate => "webview2-legacy-all-contexts-candidate",
        }
    }
}

pub(crate) fn choose_filter_mode(
    strong_interface_available: bool,
    runtime_version: &str,
) -> Result<WindowsFilterMode, &'static str> {
    if strong_interface_available {
        return Ok(WindowsFilterMode::AllSourceKinds);
    }
    let major = runtime_version
        .split('.')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or("invalid WebView2 runtime version")?;
    if major == 111 {
        Ok(WindowsFilterMode::LegacyAllContextsCandidate)
    } else {
        Err("design-change-required: source-kinds interface unavailable")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilterTuple {
    pub(crate) uri: &'static str,
    pub(crate) context: &'static str,
    pub(crate) source_kinds: Option<&'static str>,
}

impl FilterTuple {
    pub(crate) const fn all_source_kinds() -> Self {
        Self {
            uri: "*",
            context: "all",
            source_kinds: Some("all"),
        }
    }

    pub(crate) const fn legacy_all_contexts() -> Self {
        Self {
            uri: "*",
            context: "all",
            source_kinds: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SyntheticResponse {
    pub(crate) status: u16,
    pub(crate) reason: &'static str,
    pub(crate) headers: &'static str,
}

#[cfg(test)]
pub(crate) fn synthetic_response_for(raw_url: &str) -> Option<SyntheticResponse> {
    match super::classify_resource_request(raw_url) {
        ResourceRequestDecision::Allow => None,
        ResourceRequestDecision::Block { .. } => Some(SyntheticResponse {
            status: 403,
            reason: "Forbidden",
            headers: "Content-Length: 0\r\nCache-Control: no-store",
        }),
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct PolicyInstallModel {
    filter: Option<FilterTuple>,
    handler_token: Option<i64>,
    removed_filter: Option<FilterTuple>,
    removed_handler_token: Option<i64>,
    events: Vec<&'static str>,
}

#[cfg(test)]
impl PolicyInstallModel {
    pub(crate) fn filter_registered(&mut self, tuple: FilterTuple) -> Result<(), &'static str> {
        if self.filter.replace(tuple).is_some() {
            return Err("resource filter already registered");
        }
        self.events.push("filter-registered");
        Ok(())
    }

    pub(crate) fn handler_registered(&mut self, token: i64) -> Result<(), &'static str> {
        if self.filter.is_none() || self.handler_token.replace(token).is_some() {
            return Err("handler registration ordering violation");
        }
        self.events.push("handler-registered");
        Ok(())
    }

    pub(crate) fn uninstall(&mut self) -> Result<(), &'static str> {
        let filter = self.filter.take().ok_or("resource filter is missing")?;
        self.removed_filter = Some(filter);
        self.events.push("filter-removed");
        let token = self
            .handler_token
            .take()
            .ok_or("resource handler is missing")?;
        self.removed_handler_token = Some(token);
        self.events.push("handler-removed");
        Ok(())
    }

    pub(crate) fn removed_filter(&self) -> Option<&FilterTuple> {
        self.removed_filter.as_ref()
    }

    pub(crate) fn removed_handler_token(&self) -> Option<i64> {
        self.removed_handler_token
    }

    pub(crate) fn events(&self) -> &[&'static str] {
        &self.events
    }
}

pub(crate) struct WindowsResourcePolicyGuard {
    webview: ICoreWebView2,
    strong_webview: Option<ICoreWebView2_22>,
    event_token: i64,
    filter: FilterTuple,
    installed: bool,
    metadata: ResourcePolicyMetadata,
}

impl WindowsResourcePolicyGuard {
    pub(crate) fn install(
        raw_webview: &wry::WebView,
        allowed_origin: Url,
        counters: Arc<IsolationCounters>,
        fault_callback: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, SignatureError> {
        let environment = raw_webview.environment();
        let webview = raw_webview.webview();

        let mut raw_version = PWSTR::null();
        unsafe { environment.BrowserVersionString(&mut raw_version) }
            .map_err(|_| SignatureError::Webview("WebView2 version query failed".into()))?;
        let runtime_version = webview2_com::take_pwstr(raw_version);
        let strong_webview = webview.cast::<ICoreWebView2_22>().ok();
        let mode = choose_filter_mode(strong_webview.is_some(), &runtime_version)
            .map_err(|message| SignatureError::Webview(message.into()))?;
        let filter = match mode {
            WindowsFilterMode::AllSourceKinds => FilterTuple::all_source_kinds(),
            WindowsFilterMode::LegacyAllContextsCandidate => FilterTuple::legacy_all_contexts(),
        };
        let uri = HSTRING::from(filter.uri);
        match (&strong_webview, mode) {
            (Some(strong), WindowsFilterMode::AllSourceKinds) => unsafe {
                strong.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    &uri,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                )
            },
            (_, WindowsFilterMode::LegacyAllContextsCandidate) => unsafe {
                webview.AddWebResourceRequestedFilter(&uri, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
            },
            _ => Err(windows_core::Error::from_win32()),
        }
        .map_err(|_| SignatureError::Webview("WebView2 request filter failed".into()))?;

        let handler_environment = environment.clone();
        let handler_counters = Arc::clone(&counters);
        let event_handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let result = (|| {
                let args = args.ok_or_else(windows_core::Error::from_win32)?;
                let request = unsafe { args.Request()? };
                let mut raw_uri = PWSTR::null();
                unsafe { request.Uri(&mut raw_uri)? };
                let request_uri = webview2_com::take_pwstr(raw_uri);

                let decision = classify_resource_request_for(&allowed_origin, &request_uri);
                if !matches!(decision, ResourceRequestDecision::Allow) {
                    let response_spec = SyntheticResponse {
                        status: 403,
                        reason: "Forbidden",
                        headers: "Content-Length: 0\r\nCache-Control: no-store",
                    };
                    let canary =
                        matches!(decision, ResourceRequestDecision::Block { canary: true });
                    handler_counters.blocked_resource_request(canary);
                    let reason = HSTRING::from(response_spec.reason);
                    let headers = HSTRING::from(response_spec.headers);
                    let response = unsafe {
                        handler_environment.CreateWebResourceResponse(
                            None,
                            i32::from(response_spec.status),
                            &reason,
                            &headers,
                        )?
                    };
                    unsafe { args.SetResponse(&response)? };
                }
                Ok::<(), windows_core::Error>(())
            })();

            if result.is_err() {
                handler_counters.policy_fault();
                fault_callback();
            }
            Ok(())
        }));
        let mut event_token = 0;
        if unsafe { webview.add_WebResourceRequested(&event_handler, &mut event_token) }.is_err() {
            let _ = remove_filter(&webview, strong_webview.as_ref(), mode, &uri);
            return Err(SignatureError::Webview(
                "WebView2 request handler failed".into(),
            ));
        }

        Ok(Self {
            webview,
            strong_webview,
            event_token,
            filter,
            installed: true,
            metadata: ResourcePolicyMetadata {
                runtime_version,
                mode: mode.as_str().to_string(),
                strong_source_kinds_interface_available: mode == WindowsFilterMode::AllSourceKinds,
            },
        })
    }

    pub(crate) fn metadata(&self) -> &ResourcePolicyMetadata {
        &self.metadata
    }

    pub(crate) fn uninstall(&mut self) -> Result<(), SignatureError> {
        if !self.installed {
            return Ok(());
        }
        let mode = if self.filter.source_kinds.is_some() {
            WindowsFilterMode::AllSourceKinds
        } else {
            WindowsFilterMode::LegacyAllContextsCandidate
        };
        let uri = HSTRING::from(self.filter.uri);
        let filter_result = remove_filter(&self.webview, self.strong_webview.as_ref(), mode, &uri);
        let handler_result = unsafe { self.webview.remove_WebResourceRequested(self.event_token) };
        self.installed = false;
        filter_result
            .and(handler_result)
            .map_err(|_| SignatureError::Webview("WebView2 policy cleanup failed".into()))
    }
}

pub(crate) fn counterfactual_metadata(
    raw_webview: &wry::WebView,
) -> Result<ResourcePolicyMetadata, SignatureError> {
    let environment = raw_webview.environment();
    let webview = raw_webview.webview();
    let mut raw_version = PWSTR::null();
    unsafe { environment.BrowserVersionString(&mut raw_version) }
        .map_err(|_| SignatureError::Webview("WebView2 version query failed".into()))?;
    let runtime_version = webview2_com::take_pwstr(raw_version);
    Ok(ResourcePolicyMetadata {
        runtime_version,
        mode: "counterfactual-no-resource-rule".into(),
        strong_source_kinds_interface_available: webview.cast::<ICoreWebView2_22>().is_ok(),
    })
}

impl Drop for WindowsResourcePolicyGuard {
    fn drop(&mut self) {
        let _ = self.uninstall();
    }
}

fn remove_filter(
    webview: &ICoreWebView2,
    strong_webview: Option<&ICoreWebView2_22>,
    mode: WindowsFilterMode,
    uri: &HSTRING,
) -> windows_core::Result<()> {
    match (strong_webview, mode) {
        (Some(strong), WindowsFilterMode::AllSourceKinds) => unsafe {
            strong.RemoveWebResourceRequestedFilterWithRequestSourceKinds(
                uri,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )
        },
        (_, WindowsFilterMode::LegacyAllContextsCandidate) => unsafe {
            webview.RemoveWebResourceRequestedFilter(uri, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
        },
        _ => Err(windows_core::Error::from_win32()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FilterTuple, PolicyInstallModel, SyntheticResponse, WindowsFilterMode, choose_filter_mode,
        classify_resource_request_for, synthetic_response_for,
    };
    use url::Url;

    #[test]
    fn signature_webview_windows_mode_prefers_source_kinds_and_limits_legacy_to_111() {
        assert_eq!(
            choose_filter_mode(true, "111.0.1661.62").unwrap(),
            WindowsFilterMode::AllSourceKinds
        );
        assert_eq!(
            choose_filter_mode(false, "111.0.1661.62").unwrap(),
            WindowsFilterMode::LegacyAllContextsCandidate
        );
        assert!(choose_filter_mode(false, "112.0.1722.34").is_err());
        assert!(choose_filter_mode(false, "not-a-version").is_err());
    }

    #[test]
    fn signature_webview_windows_uninstall_uses_exact_tuple_before_handler_token() {
        let tuple = FilterTuple::all_source_kinds();
        let mut model = PolicyInstallModel::default();
        model.filter_registered(tuple.clone()).unwrap();
        model.handler_registered(42).unwrap();
        model.uninstall().unwrap();

        assert_eq!(model.removed_filter(), Some(&tuple));
        assert_eq!(model.removed_handler_token(), Some(42));
        assert_eq!(
            model.events(),
            [
                "filter-registered",
                "handler-registered",
                "filter-removed",
                "handler-removed"
            ]
        );
    }

    #[test]
    fn signature_webview_windows_policy_returns_empty_403_for_every_disallowed_request() {
        assert_eq!(
            synthetic_response_for("https://music.gdstudio.xyz/api.php"),
            None
        );
        for denied in [
            "not a url",
            "http://127.0.0.1:31337/canary",
            "https://evil.example/resource",
            "file:///yinmi-denied",
        ] {
            assert_eq!(
                synthetic_response_for(denied),
                Some(SyntheticResponse {
                    status: 403,
                    reason: "Forbidden",
                    headers: "Content-Length: 0\r\nCache-Control: no-store",
                })
            );
        }
    }

    #[test]
    fn signature_webview_windows_native_callback_classifies_the_injected_exact_origin() {
        let allowed = Url::parse("https://127.0.0.1:54321/").unwrap();
        assert_eq!(
            classify_resource_request_for(&allowed, "https://127.0.0.1:54321/redirect/one"),
            super::ResourceRequestDecision::Allow
        );
        for denied in [
            "https://127.0.0.1:54322/blocked/fetch",
            "wss://127.0.0.1:54322/ws/websocket",
            "file:///yinmi-feasibility-denied",
        ] {
            assert!(matches!(
                classify_resource_request_for(&allowed, denied),
                super::ResourceRequestDecision::Block { .. }
            ));
        }
        assert_eq!(
            classify_resource_request_for(&allowed, "https://127.0.0.1:54322/blocked/fetch"),
            super::ResourceRequestDecision::Block { canary: true }
        );
    }
}
