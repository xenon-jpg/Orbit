import { createContext, useContext } from 'react';
import type { Account, Page, Snapshot, VaultStatus } from './types';
import type { TextKey } from './i18n';
export interface UIContext {
  snapshot: Snapshot; status: VaultStatus; page: Page; navigate: (page: Page) => void;
  selected: string[]; setSelected: (ids: string[]) => void; target: string; setTarget: (value: string) => void;
  busy: boolean; refresh: () => Promise<void>; notify: (message: string, error?: boolean) => void;
  run: (task: () => Promise<unknown>, message?: string) => Promise<boolean>;
  t: (key: TextKey) => string; addAccount: () => void; editAccount: (account: Account) => void;
  launch: (ids?: string[], target?: string) => Promise<void>;
  sameServer: boolean; setSameServer: (value: boolean) => void;
  followFirst: boolean; setFollowFirst: (value: boolean) => void;
  lock: () => Promise<void>;
}
export const AppContext = createContext<UIContext | null>(null);
export function useApp() { const context = useContext(AppContext); if (!context) throw new Error('Missing app context'); return context; }
