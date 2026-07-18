import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import App from './App.svelte';

describe('application shell', () => {
  it('shows the confirmed product identity and phase', () => {
    render(App);
    expect(screen.getByRole('heading', { name: '音觅' })).toBeTruthy();
    expect(screen.getByText('第一阶段可行性验证')).toBeTruthy();
    expect(
      screen.queryByRole('region', { name: '签名可行性控制台' }),
    ).toBeNull();
  });
});
