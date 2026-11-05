export type Page = 'home' | 'accounts' | 'play' | 'servers' | 'performance' | 'monitor' | 'alerts' | 'settings' | 'diagnostics';
export type Target = { kind: 'place'; placeId: number } | { kind: 'job'; placeId: number; jobId: string } | { kind: 'private'; placeId: number; linkCode: string } | { kind: 'reserved'; placeId: number; accessCode: string } | { kind: 'follow'; userId: number };
export interface Performance {
  preset: string; fps: number; textureQuality: number; disableGrass: boolean; graySky: boolean;
  disableShadows: boolean; cpuPercent: number; fairCpu: boolean; trimMemory: boolean; tinyWindows: boolean;
}
export interface Settings {
  language: 'ru' | 'en'; theme: 'dark' | 'light'; accent: string; background: string;
  autoLockMinutes: number; multiInstance: boolean; launchDelaySeconds: number; maxClients: number;
  autoRejoin: boolean; autoRestart: boolean; restartFrozen: boolean; frozenAfterSeconds: number;
  memoryLimitMb: number; antiAfk: boolean; antiAfkMinutes: number; tileColumns: number;
  startSmall: boolean; protectSharedLogin: boolean; checkRobloxUpdates: boolean;
  webhookEnabled: boolean; performance: Performance;
}
export interface Account {
  id: string; userId: number; username: string; displayName: string; alias: string; note: string;
  favorite: boolean; avatarUrl: string; robux: number | null; presence: number; friends: number | null;
  followers: number | null; premium: boolean | null; groups: string[]; sessionStatus: string;
  checkedAt: number | null; fps: number | null;
}
export interface Client {
  id: string; accountId: string; pid: number; state: string; reason: string; startedAt: number;
  cpu: number; memoryMb: number; commitMb: number; gpu: number | null; target: Target;
  gameName: string; restarts: number; lastActivity: number; windowFound: boolean; minimized: boolean;
}
export interface Batch { id: string; total: number; completed: number; state: string; currentAccount: string; error: string }
export interface Favorite { id: string; name: string; target: string; icon: string }
export interface HistoryEntry { accountId: string; target: Target; at: number }
export interface Snapshot { accounts: Account[]; settings: Settings; favorites: Favorite[]; history: HistoryEntry[]; clients: Client[]; batches: Batch[] }
export interface VaultStatus { exists: boolean; unlocked: boolean; mode: string; hasRecovery: boolean; helloAvailable: boolean; platform: string; version: string }
export interface GameInfo { placeId: number; universeId: number; name: string; description: string; icon: string; playing: number; visits: number; maxPlayers: number }
export interface ServerInfo { id: string; playing: number; maxPlayers: number; ping: number | null; fps: number | null; private: boolean; name: string; linkCode: string }
export interface ServerPage { servers: ServerInfo[]; nextCursor: string | null }
export interface LaunchOptions { accountIds: string[]; target: string; sameServer: boolean; followFirst: boolean; performance?: Performance }
export interface DiagnosticEvent { at: number; code: string; message: string }
