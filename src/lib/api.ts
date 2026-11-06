import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Account, Performance, Settings, Snapshot, VaultStatus, LaunchOptions, GameInfo, ServerPage, DiagnosticEvent, Favorite, Target } from './types';
import { initialSnapshot, saveDemo, defaultSettings, avatar, demoClient } from './demo';

export const native = isTauri();
let demo = native ? null : initialSnapshot();
const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
function local(): Snapshot { if (!demo) throw new Error('Native snapshot unavailable'); return demo; }
function commit() { if (demo) saveDemo(demo); }
function account(id: string): Account { const a = local().accounts.find(a => a.id === id); if (!a) throw new Error('Аккаунт не найден'); return a; }

export const api = {
  async status(): Promise<VaultStatus> {
    if (native) return invoke('vault_status');
    return { exists: true, unlocked: true, mode: 'demo', hasRecovery: false, helloAvailable: false, platform: 'browser', version: '0.2.1' };
  },
  async initialize(password?: string) { if (native) return invoke('vault_initialize', { password: password || null }); },
  async unlock(password: string) { if (native) return invoke('vault_unlock', { password }); },
  async lock() { if (native) return invoke('vault_lock'); local().clients = []; },
  async recover(code: string, password: string) { if (native) return invoke('vault_recover', { code, password }); },
  async setPassword(current: string | undefined, password: string, recovery: boolean): Promise<string | null> {
    if (native) return invoke('vault_set_password', { current: current || null, password, recovery });
    if (password.length < 12) throw new Error('Минимум 12 символов');
    return recovery ? 'DEMO-ONLY-NOT-A-RECOVERY-CODE' : null;
  },
  async removePassword(current: string) { if (native) return invoke('vault_remove_password', { current }); },
  async touch() { if (native) return invoke('vault_touch'); },
  async backup(path: string) { if (native) return invoke('vault_backup', { path }); },
  async snapshot(): Promise<Snapshot> {
    if (native) return invoke('snapshot');
    return structuredClone(local());
  },
  async loginBegin(): Promise<string> {
    if (native) return invoke('login_begin');
    await wait(400); return 'demo-login';
  },
  async loginFinish(label: string): Promise<Account> {
    if (native) return invoke('login_finish', { label });
    const n = local().accounts.length + 1;
    const a: Account = { id: `demo-${Date.now()}`, userId: 10000+n, username: `player.${n}`, displayName: `Player ${n}`, alias: '', note: '', favorite: false, avatarUrl: avatar(n), robux: 0, presence: 1, friends: 0, followers: 0, premium: false, groups: [], sessionStatus: 'valid', checkedAt: Math.floor(Date.now()/1000), fps: null };
    local().accounts.push(a); commit(); return a;
  },
  async loginCancel() { if (native) return invoke('login_cancel'); },
  async importAccount(cookie: string, alias: string): Promise<Account> {
    if (native) return invoke('account_import', { cookie, alias });
    if (cookie.trim()) throw new Error('Демо-режим не принимает cookies. Используйте вход с демо-аккаунтом.');
    return this.loginFinish('demo-login');
  },
  async refresh(id: string): Promise<Account> {
    if (native) return invoke('account_refresh', { id });
    await wait(450); const a = account(id); a.checkedAt = Math.floor(Date.now()/1000); commit(); return a;
  },
  async updateAccount(id: string, changes: { alias: string; note: string; favorite: boolean; fps: number | null }) {
    if (native) return invoke('account_update', { id, ...changes });
    Object.assign(account(id), changes); commit();
  },
  async removeAccount(id: string) {
    if (native) return invoke('account_remove', { id });
    local().accounts = local().accounts.filter(a => a.id !== id); local().clients = local().clients.filter(c => c.accountId !== id); commit();
  },
  async reveal(id: string, password?: string): Promise<string> {
    if (native) return invoke('account_reveal', { id, password: password || null });
    throw new Error('В демо-режиме нет настоящих сессий.');
  },
  async copyCookie(id: string, password?: string) {
    if (native) return invoke('account_copy', { id, password: password || null });
    throw new Error('В демо-режиме нет настоящих сессий.');
  },
  async resolve(input: string): Promise<Target> {
    if (native) return invoke('resolve_target', { input });
    const placeId = Number(input.match(/(?:games\/|^)(\d+)/)?.[1]);
    if (!placeId) throw new Error('В демо укажите числовой Place ID, например 920587237.');
    return { kind: 'place', placeId };
  },
  async game(placeId: number): Promise<GameInfo> {
    if (native) return invoke('game_info', { placeId });
    await wait(250);
    return { placeId, universeId: 1, name: placeId === 4924922222 ? 'Brookhaven RP' : placeId === 920587237 ? 'Adopt Me!' : `Experience ${placeId}`, description: 'Демонстрационные данные. В приложении здесь будет информация из Roblox.', icon: '', playing: 24891, visits: 1200000, maxPlayers: 48 };
  },
  async servers(accountId: string | undefined, placeId: number, privateServers: boolean, cursor?: string, sort?: string): Promise<ServerPage> {
    if (native) return invoke('server_list', { accountId: accountId || null, placeId, private: privateServers, cursor: cursor || null, sort: sort || 'desc' });
    await wait(350);
    return { servers: Array.from({length: 6}, (_, i) => ({ id: `demo-server-${i}`, playing: privateServers ? 0 : sort === 'asc' ? 3+i*2 : 42-i*4, maxPlayers: 48, ping: 38+i*12, fps: 59.8, private: privateServers, name: `Сервер ${i+1}`, linkCode: '' })), nextCursor: null };
  },
  async launch(options: LaunchOptions): Promise<string> {
    if (native) return invoke('launch_batch', { options });
    if (!options.accountIds.length) throw new Error('Выберите хотя бы один аккаунт.');
    await this.resolve(options.target);
    const id = `batch-${Date.now()}`;
    local().batches.push({id, total: options.accountIds.length, completed: 0, state: 'running', currentAccount: options.accountIds[0], error: ''});
    void (async () => {
      for (const accountId of options.accountIds) {
        await wait(800);
        const batch = local().batches.find(b => b.id === id);
        if (!batch || batch.state === 'cancelled') break;
        if (!local().clients.some(c => c.accountId === accountId)) local().clients.push(demoClient(account(accountId), local().clients.length));
        batch.completed++; batch.currentAccount = accountId; commit();
      }
      const batch = local().batches.find(b => b.id === id);
      if (batch && batch.state !== 'cancelled') batch.state = 'complete';
      commit();
    })();
    return id;
  },
  async cancelLaunch(id: string) { if (native) return invoke('launch_cancel', { id }); const b = local().batches.find(b => b.id === id); if (b) b.state = 'cancelled'; },
  async clientAction(id: string, action: string) {
    if (native) return invoke('client_action', { id, action });
    const client = local().clients.find(c => c.id === id); if (!client) return;
    if (action === 'close' || action === 'force-close') local().clients = local().clients.filter(c => c.id !== id);
    if (action === 'minimize') client.minimized = true;
    if (action === 'focus') client.minimized = false;
    if (action === 'trim') client.memoryMb = Math.round(client.memoryMb * .75);
    commit();
  },
  async preview(id: string): Promise<string> { if (native) return invoke('client_preview', { id }); throw new Error('demo'); },
  async stopAll() { if (native) return invoke('stop_all'); local().clients = []; local().batches.forEach(b => b.state = 'cancelled'); commit(); },
  async tile() { if (native) return invoke('tile_clients'); },
  async saveSettings(settings: Settings) {
    if (native) return invoke('settings_save', { settings });
    local().settings = structuredClone(settings); commit();
  },
  async applyPerformance(profile: Performance) { if (native) return invoke('performance_apply', { profile }); local().settings.performance = profile; commit(); },
  async saveFavorite(name: string, target: string): Promise<Favorite> {
    if (native) return invoke('favorite_save', { name, input: target });
    const favorite = { id: `fav-${Date.now()}`, name, target, icon: '' }; local().favorites.push(favorite); commit(); return favorite;
  },
  async removeFavorite(id: string) { if (native) return invoke('favorite_remove', { id }); local().favorites = local().favorites.filter(f => f.id !== id); commit(); },
  async saveWebhook(url: string, enabled: boolean) {
    if (native) return invoke('webhook_save', { url, enabled });
    if (url) throw new Error('Не вводите настоящий webhook в демо. Настройки интеграции доступны в desktop-версии.');
    local().settings.webhookEnabled = false; commit();
  },
  async testWebhook() { if (native) return invoke('webhook_test'); throw new Error('Доступно в приложении для Windows.'); },
  async diagnostics(): Promise<DiagnosticEvent[]> { if (native) return invoke('diagnostics'); return [{at:Math.floor(Date.now()/1000),code:'demo_mode',message:'Browser demo. No Roblox requests, cookies or processes.'}]; },
  async robloxStatus(): Promise<{installed: boolean; path: string | null; platformSupported: boolean}> { if (native) return invoke('roblox_status'); return {installed:false,path:null,platformSupported:false}; },
  async downloadRoblox() { if (native) return invoke('open_roblox_download'); window.open('https://www.roblox.com/download', '_blank', 'noopener,noreferrer'); },
  async onChange(callback: () => void): Promise<() => void> {
    if (!native) return () => {};
    const unlisten = await listen('manager-changed', callback);
    const locked = await listen('vault-locked', callback);
    return () => { unlisten(); locked(); };
  },
};
export { defaultSettings };
