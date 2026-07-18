<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  export let disabled = false;

  type ProbeCase = 'single_count1000' | 'paged_official20' | 'repeat_same_page';

  let probeCase: ProbeCase = 'single_count1000';
  let running = false;
  let output = 'No live request has been started.';

  async function runProbe() {
    running = true;
    output = 'Running the fixed, rate-limited probe…';
    try {
      const report = await invoke('feasibility_run_gd_probe', {
        probeCase,
      });
      output = JSON.stringify(report, null, 2);
    } catch (error) {
      output = `Probe failed: ${String(error)}`;
    } finally {
      running = false;
    }
  }
</script>

<section class="probe" aria-labelledby="gd-probe-title">
  <div>
    <span class="eyebrow">LIVE / GD PROTOCOL</span>
    <h3 id="gd-probe-title">Fixed upstream request</h3>
    <p>
      Only the reviewed keyword, source, count and pacing plans are available.
    </p>
  </div>

  <label>
    Probe case
    <select bind:value={probeCase} disabled={disabled || running}>
      <option value="single_count1000">Single count 1000</option>
      <option value="paged_official20">Official 20 × pages 1–50</option>
      <option value="repeat_same_page">Repeat page 1 twice</option>
    </select>
  </label>

  <button class="danger" disabled={disabled || running} onclick={runProbe}>
    {running ? 'Running…' : 'Run live probe'}
  </button>

  <pre aria-live="polite">{output}</pre>
</section>

<style>
  .probe {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) minmax(230px, 0.8fr) auto;
    gap: 16px;
    align-items: end;
    border: 1px solid #b8c0c8;
    border-left: 5px solid #ba4c2f;
    background: #f4f1ea;
    padding: 18px;
  }

  .eyebrow,
  label,
  pre {
    font-family: ui-monospace, 'Cascadia Code', monospace;
  }

  .eyebrow {
    color: #9a3b26;
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.14em;
  }

  h3 {
    margin: 4px 0;
    color: #17202a;
    font-size: 1rem;
  }

  p {
    margin: 0;
    color: #59616a;
    font-size: 0.82rem;
  }

  label {
    display: grid;
    gap: 6px;
    color: #3d4751;
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  select,
  button {
    min-height: 38px;
    border: 1px solid #89939c;
    font: inherit;
  }

  select {
    background: #fffdf8;
    padding: 0 10px;
  }

  button {
    cursor: pointer;
    background: #17202a;
    color: #fff;
    padding: 0 14px;
  }

  button.danger {
    background: #9a3b26;
    border-color: #7e2f1f;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.48;
  }

  pre {
    grid-column: 1 / -1;
    max-height: 190px;
    overflow: auto;
    margin: 0;
    background: #17202a;
    color: #d8e3df;
    padding: 12px;
    font-size: 0.72rem;
    line-height: 1.5;
    white-space: pre-wrap;
  }

  @media (max-width: 820px) {
    .probe {
      grid-template-columns: 1fr;
    }
  }
</style>
