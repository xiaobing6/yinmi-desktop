<script lang="ts">
  import { invoke, type InvokeArgs } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import GdProbe from './GdProbe.svelte';

  type RuntimeState = 'idle' | 'working' | 'ready' | 'failed';

  let state: RuntimeState = 'idle';
  let signInput = 'type=search&name=%E5%91%A8%E6%9D%B0%E4%BC%A6';
  let readout = 'Awaiting command.';
  let canaryReady = false;

  onMount(() => {
    void establishCanary();
  });

  async function establishCanary() {
    try {
      const result = await invoke<{ count: number }>('feasibility_ipc_canary');
      canaryReady = result.count > 0;
    } catch (error) {
      state = 'failed';
      readout = `Main-window IPC canary failed: ${String(error)}`;
    }
  }

  async function execute<T>(label: string, command: string, args?: InvokeArgs) {
    state = 'working';
    readout = `${label}…`;
    try {
      const result = await invoke<T>(command, args);
      state = command.endsWith('destroy') ? 'idle' : 'ready';
      readout = JSON.stringify(result ?? { status: 'ok' }, null, 2);
    } catch (error) {
      state = 'failed';
      readout = `${label} failed: ${String(error)}`;
    }
  }
</script>

<section class="panel" aria-label="签名可行性控制台">
  <div class="masthead">
    <div>
      <span class="kicker">RAW WRY / ISOLATED SIGNATURE HOST</span>
      <h2>Signature feasibility workbench</h2>
    </div>
    <div class:bad={state === 'failed'} class="state" data-state={state}>
      <span></span>{state}
    </div>
  </div>

  <div class="grid">
    <section class="controls" aria-labelledby="runtime-controls-title">
      <div class="section-number">01</div>
      <div>
        <h3 id="runtime-controls-title">Runtime controls</h3>
        <p>
          The raw child has no application command bridge or matching
          capability.
        </p>
      </div>
      <div class="button-row">
        <button
          disabled={state === 'working'}
          onclick={() =>
            execute('Initializing', 'feasibility_signature_initialize')}
          >Initialize</button
        >
        <button
          disabled={state === 'working'}
          onclick={() => execute('Destroying', 'feasibility_signature_destroy')}
          >Destroy</button
        >
        <button
          disabled={state === 'working' || !canaryReady}
          onclick={() =>
            execute(
              'Running isolation seams',
              'feasibility_signature_isolation',
            )}>Isolation suite</button
        >
      </div>
    </section>

    <section class="controls" aria-labelledby="signature-control-title">
      <div class="section-number">02</div>
      <div>
        <h3 id="signature-control-title">Signature call</h3>
        <p>
          Input is encoded by Rust before calling the official page function.
        </p>
      </div>
      <label>
        Probe input
        <textarea bind:value={signInput} rows="3"></textarea>
      </label>
      <button
        disabled={state === 'working' || signInput.length === 0}
        onclick={() =>
          execute('Signing', 'feasibility_signature_sign', {
            input: signInput,
          })}>Call crc32</button
      >
    </section>

    <section class="readout" aria-labelledby="readout-title">
      <div class="readout-title">
        <span id="readout-title">COMMAND READOUT</span>
        <span>{canaryReady ? 'MAIN IPC BASELINE OK' : 'CANARY PENDING'}</span>
      </div>
      <pre aria-live="polite">{readout}</pre>
    </section>
  </div>

  <GdProbe disabled={state === 'working' || state !== 'ready'} />
</section>

<style>
  .panel {
    display: grid;
    gap: 18px;
    max-width: 1180px;
    margin: 0 auto;
    color: #17202a;
  }

  .masthead,
  .controls,
  .readout-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .masthead {
    gap: 16px;
    background: #17202a;
    color: #f8f5ed;
    padding: 20px 22px;
  }

  .kicker,
  .section-number,
  .state,
  .readout-title,
  label,
  pre {
    font-family: ui-monospace, 'Cascadia Code', monospace;
  }

  .kicker {
    color: #ef7b55;
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.16em;
  }

  h2 {
    margin: 5px 0 0;
    font-size: clamp(1.25rem, 3vw, 2rem);
    font-weight: 600;
    letter-spacing: -0.025em;
  }

  .state {
    gap: 8px;
    border: 1px solid #60717c;
    padding: 7px 10px;
    color: #d8e3df;
    font-size: 0.7rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .state span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #68b39b;
  }

  .state.bad span {
    background: #ef7b55;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }

  .controls {
    align-content: start;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: 14px;
    border: 1px solid #b8c0c8;
    background: #fffdf8;
    padding: 18px;
  }

  .section-number {
    color: #ba4c2f;
    font-size: 0.72rem;
    font-weight: 700;
  }

  h3 {
    margin: 0 0 4px;
    font-size: 0.96rem;
  }

  p {
    margin: 0;
    color: #626b74;
    font-size: 0.8rem;
  }

  .button-row {
    display: flex;
    flex: 1 0 100%;
    gap: 8px;
  }

  button {
    min-height: 38px;
    cursor: pointer;
    border: 1px solid #626d76;
    background: #17202a;
    color: #fff;
    padding: 0 14px;
    font:
      0.76rem ui-monospace,
      'Cascadia Code',
      monospace;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.42;
  }

  label {
    display: grid;
    flex: 1 0 100%;
    gap: 6px;
    color: #45505a;
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  textarea {
    box-sizing: border-box;
    width: 100%;
    resize: vertical;
    border: 1px solid #89939c;
    background: #f7f5ef;
    padding: 9px;
    color: #17202a;
    font:
      0.78rem/1.5 ui-monospace,
      'Cascadia Code',
      monospace;
  }

  .readout {
    grid-column: 1 / -1;
    border: 1px solid #46545e;
    background: #202b33;
  }

  .readout-title {
    padding: 9px 12px;
    border-bottom: 1px solid #46545e;
    color: #aab9bd;
    font-size: 0.66rem;
    letter-spacing: 0.09em;
  }

  pre {
    box-sizing: border-box;
    min-height: 118px;
    max-height: 260px;
    overflow: auto;
    margin: 0;
    padding: 14px;
    color: #d8e3df;
    font-size: 0.74rem;
    line-height: 1.55;
    white-space: pre-wrap;
  }

  @media (max-width: 760px) {
    .grid {
      grid-template-columns: 1fr;
    }

    .masthead {
      align-items: flex-start;
      flex-direction: column;
    }

    .button-row {
      flex-wrap: wrap;
    }
  }
</style>
