import type { Account, Snapshot, Settings, Performance, Client } from './types';

export const defaultPerformance: Performance = { preset: 'balanced', fps: 30, textureQuality: 2, disableGrass: false, graySky: false, disableShadows: false, cpuPercent: 0, fairCpu: false, trimMemory: false, tinyWindows: false };
export const defaultSettings: Settings = {
  language: 'ru', theme: 'dark', accent: '#c3f36b', background: '', autoLockMinutes: 15,
  multiInstance: true, launchDelaySeconds: 12, maxClients: 10, autoRejoin: true, autoRestart: false,
  restartFrozen: false, frozenAfterSeconds: 300, memoryLimitMb: 0, antiAfk: false, antiAfkMinutes: 8,
  tileColumns: 3, startSmall: false, protectSharedLogin: true, checkRobloxUpdates: false,
  webhookEnabled: false, performance: defaultPerformance,
};
export function avatar(seed: number, color = 'b6caef') {
  const hair = ['#513d32', '#d4b178', '#343438', '#854f32'][seed % 4];
  const skin = ['#e7b88f', '#f2cca9', '#bb855e', '#efc4a3'][seed % 4];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="160" height="160" viewBox="0 0 160 160"><rect width="160" height="160" rx="80" fill="#${color}"/><path d="M24 160v-24c0-28 25-42 56-42s56 14 56 42v24" fill="${['#334c45','#424b74','#493c3b','#263e55'][seed%4]}"/><path d="M65 94h30v27H65" fill="${skin}"/><rect x="45" y="33" width="70" height="72" rx="25" fill="${skin}"/><path d="M43 66V43c0-28 74-37 74 6v24l-13-24c-17 13-36 6-45 1l-9 21z" fill="${hair}"/><circle cx="65" cy="73" r="3" fill="#29282a"/><circle cx="95" cy="73" r="3" fill="#29282a"/><path d="M70 87q10 9 20 0" fill="none" stroke="#754e3c" stroke-width="3" stroke-linecap="round"/><path d="M53 118l27 15 27-15" fill="none" stroke="#ffffff30" stroke-width="4"/></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
const names = [ ['Alex', 'alex_nova', 'a6c6ae'], ['Mira', 'mira.cloud', 'c5b5d9'], ['Max', 'maxventure', 'd6be9f'], ['Luna', 'luna_play', 'a9c5d8'], ['Kai', 'kai.orbit', 'c9b0a7'], ['Noah', 'noah.gg', 'b4bda1'] ];
export const demoAccounts: Account[] = names.map(([name, username, color], index) => ({
  id: `demo-${index + 1}`, userId: 10001 + index, username, displayName: name, alias: index === 0 ? 'Основной' : '', note: '', favorite: index < 2,
  avatarUrl: avatar(index, color), robux: [2450, 820, 1350, 640, 0, 210][index], presence: [2, 2, 1, 0, 0, 1][index],
  friends: [128, 64, 85, 31, 12, 46][index], followers: [240, 90, 103, 42, 15, 59][index], premium: index === 0,
  groups: ['AltDeck Community', 'Builders Club'], sessionStatus: 'valid', checkedAt: Math.floor(Date.now()/1000), fps: null,
}));
export function demoClient(account: Account, index: number): Client {
  return { id: `client-${account.id}`, accountId: account.id, pid: 12040 + index * 328, state: 'running', reason: '', startedAt: Math.floor(Date.now()/1000) - [5034, 4872, 3201][index%3], cpu: [8.2, 6.4, 7.1][index%3], memoryMb: [842, 756, 810][index%3], commitMb: 1100, gpu: null, target: {kind:'place',placeId:920587237}, gameName: 'Adopt Me!', restarts: 0, lastActivity: Math.floor(Date.now()/1000), windowFound: true, minimized: false };
}
export function initialSnapshot(): Snapshot {
  try {
    const saved = JSON.parse(localStorage.getItem('altdeck-demo-v1') || 'null');
    if (saved && Array.isArray(saved.accounts) && saved.settings && saved.accounts.every((a: Account) => typeof a.id === 'string' && a.id.startsWith('demo-'))) {
      return { ...saved, clients: saved.accounts.slice(0,3).map(demoClient), batches: [], settings: { ...defaultSettings, ...saved.settings, performance: { ...defaultPerformance, ...saved.settings.performance } } };
    }
  } catch { /* A malformed browser demo must not prevent startup. */ }
  return { accounts: demoAccounts, settings: defaultSettings, favorites: [
    { id: 'fav-1', name: 'Adopt Me!', target: '920587237', icon: '' },
    { id: 'fav-2', name: 'Brookhaven RP', target: '4924922222', icon: '' },
  ], history: [], clients: demoAccounts.slice(0,3).map(demoClient), batches: [] };
}
export function saveDemo(snapshot: Snapshot) {
  // The browser adapter never accepts credentials, even in its serialized state.
  const { accounts, settings, favorites, history } = snapshot;
  localStorage.setItem('altdeck-demo-v1', JSON.stringify({ accounts, settings, favorites, history }));
}
