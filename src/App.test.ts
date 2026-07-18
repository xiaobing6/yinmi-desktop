import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import App from './App.svelte';

describe('application shell', () => {
  it('shows the product identity while the application initializes', () => {
    render(App);
    expect(screen.getByRole('heading', { name: '音觅' })).toBeTruthy();
    expect(screen.getByRole('main', { name: '正在启动音觅' })).toBeTruthy();
    expect(screen.getByText('正在准备音乐服务…')).toBeTruthy();
    expect(screen.queryByText('第一阶段可行性验证')).toBeNull();
    expect(
      screen.queryByRole('region', { name: '签名可行性控制台' }),
    ).toBeNull();
  });
});
