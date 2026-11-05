import { beforeEach, describe, expect, it } from 'vitest';
import { api } from '../api';
import { defaultSettings } from '../demo';

beforeEach(() => localStorage.clear());
describe('browser adapter safety', () => {
  it('contains only fictional accounts and no credentials', async () => {
    const snapshot = await api.snapshot();
    expect(snapshot.accounts).toHaveLength(6);
    expect(snapshot.accounts.every(a => a.id.startsWith('demo-'))).toBe(true);
    expect(JSON.stringify(snapshot)).not.toContain('ROBLOSECURITY');
    expect((await api.status()).mode).toBe('demo');
  });
  it('rejects real cookies and webhooks', async () => {
    await expect(api.importAccount('a-real-looking-cookie', '')).rejects.toThrow();
    await expect(api.saveWebhook('https://discord.com/api/webhooks/1/secret', true)).rejects.toThrow();
    await expect(api.reveal('demo-1')).rejects.toThrow();
  });
  it('persists non-secret settings and account metadata', async () => {
    await api.saveSettings({ ...defaultSettings, language: 'en', theme: 'light' });
    await api.updateAccount('demo-1', { alias: 'Test alias', note: 'private demo note', favorite: false, fps: 45 });
    const data = JSON.parse(localStorage.getItem('altdeck-demo-v1')!);
    expect(data.settings.language).toBe('en');
    expect(data.accounts[0].alias).toBe('Test alias');
    expect(data.accounts[0].fps).toBe(45);
  });
  it('runs and cancels a fictional launch queue', async () => {
    await api.stopAll();
    const id = await api.launch({ accountIds: ['demo-4'], target: '920587237', sameServer: false, followFirst: false });
    expect((await api.snapshot()).batches.at(-1)?.id).toBe(id);
    await api.cancelLaunch(id);
    expect((await api.snapshot()).batches.find(b => b.id === id)?.state).toBe('cancelled');
    await api.stopAll();
    expect((await api.snapshot()).clients).toHaveLength(0);
  });
  it('rejects malformed demo destinations', async () => {
    await expect(api.resolve('https://evil.example')).rejects.toThrow();
    expect(await api.resolve('920587237')).toEqual({ kind: 'place', placeId: 920587237 });
  });
});
