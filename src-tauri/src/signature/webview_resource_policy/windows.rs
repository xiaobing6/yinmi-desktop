use std::sync::Arc;

use super::{
    IsolationCounters, ResourcePolicyMetadata, ResourceRequestDecision,
    classify_resource_request_for,
};
use crate::signature::signature_webview::SignatureError;
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
