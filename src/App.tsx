import { useCallback, useEffect, useRef, useState } from 'react';
import { LayoutDashboard, Users, Gamepad2, Server, Gauge, Activity, Bell, Settings as SettingsIcon, Terminal, LockKeyhole, ChevronRight, ShieldCheck, ArrowUpRight, Menu, X, Check, AlertCircle, RefreshCw, Command, Github } from 'lucide-react';
import type { Account, Page, Snapshot, VaultStatus } from './lib/types';
import { api, native } from './lib/api';
import { initialSnapshot, defaultSettings } from './lib/demo';
import { text, type TextKey } from './lib/i18n';
import { AppContext } from './lib/context';
import { Avatar, Button, IconButton } from './components/ui';
import { AddAccount, EditAccount, Unlock } from './components/Dialogs';
import { Home } from './pages/Home';
import { Accounts } from './pages/Accounts';
import { Play } from './pages/Play';
import { Servers } from './pages/Servers';
import { PerformancePage } from './pages/Performance';
import { Monitor } from './pages/Monitor';
import { Alerts } from './pages/Alerts';
import { SettingsPage } from './pages/Settings';
import { Diagnostics } from './pages/Diagnostics';

const mainNav=[['home',LayoutDashboard],['accounts',Users],['play',Gamepad2],['servers',Server]] as const;
const toolNav=[['performance',Gauge],['monitor',Activity],['alerts',Bell]] as const;
const empty:Snapshot={accounts:[],settings:defaultSettings,favorites:[],history:[],clients:[],batches:[]};
const initialStatus:VaultStatus={exists:false,unlocked:false,mode:'windows',hasRecovery:false,helloAvailable:false,platform:'windows',version:'0.1.0'};
export default function App() {
  const [snapshot,setSnapshot]=useState<Snapshot>(()=>native?empty:initialSnapshot());
  const [status,setStatus]=useState<VaultStatus>(initialStatus); const [ready,setReady]=useState(!native);
  const [page,setPage]=useState<Page>('home'); const [selected,setSelected]=useState<string[]>(native?[]:['demo-1','demo-2','demo-3']);
  const [target,setTarget]=useState(native?'':'920587237'); const [sameServer,setSameServer]=useState(true); const [followFirst,setFollowFirst]=useState(false);
  const [busy,setBusy]=useState(false); const operation=useRef(false);
  const [add,setAdd]=useState(false); const [edit,setEdit]=useState<Account|null>(null); const [mobileNav,setMobileNav]=useState(false);
  const [toast,setToast]=useState<{message:string;error:boolean}|null>(null); const toastTimer=useRef<ReturnType<typeof setTimeout>|null>(null);
  const content=useRef<HTMLDivElement>(null);
  const t=useCallback((key:TextKey)=>text(snapshot.settings.language,key),[snapshot.settings.language]);
  const notify=useCallback((message:string,error=false)=>{if(toastTimer.current)clearTimeout(toastTimer.current);setToast({message,error});toastTimer.current=setTimeout(()=>setToast(null),error?8000:4000);},[]);
  const refresh=useCallback(async()=>{const value=await api.snapshot();setSnapshot(value);setStatus(await api.status());},[]);
  const run=useCallback(async(task:()=>Promise<unknown>,message?:string)=>{
    if(operation.current){notify(t('loading'));return false;}
    operation.current=true;setBusy(true);
    try{await task();if(message)notify(message);return true;}
    catch(e){notify(typeof e==='string'?e:e instanceof Error?e.message:JSON.stringify(e),true);return false;}
    finally{operation.current=false;setBusy(false);}
  },[notify,t]);
  const navigate=useCallback((next:Page)=>{setPage(next);setMobileNav(false);content.current?.scrollTo({top:0});window.scrollTo({top:0});},[]);
  const lock=useCallback(async()=>{await api.lock();setStatus(await api.status());setSnapshot(empty);setSelected([]);setEdit(null);setAdd(false);},[]);
  const launch=async(ids?:string[],destination?:string)=>{
    const accountIds=ids??selected;const input=destination??target;
    if(!accountIds.length||!input){notify(t('chooseAccounts'),true);return;}
    await run(async()=>{await api.launch({accountIds,target:input,sameServer,followFirst});await refresh();},native?t('queue'):t('demoAction'));
  };
  useEffect(()=>{
    let active=true;
    void (async()=>{try{const s=await api.status();if(!active)return;setStatus(s);if(s.unlocked){const data=await api.snapshot();if(active)setSnapshot(data);}}catch(e){if(active)notify(String(e),true);}finally{if(active)setReady(true);}})();
    return()=>{active=false;};
  },[notify]);
  useEffect(()=>{
    if(!ready||!status.unlocked)return;
    let active=true,unlisten:(()=>void)|undefined;
    void api.onChange(()=>{if(active)void refresh().catch(()=>{});}).then(fn=>{if(active)unlisten=fn;else fn();});
    const timer=setInterval(()=>{if(active)void refresh().catch(()=>{});},2000);
    return()=>{active=false;clearInterval(timer);unlisten?.();};
  },[ready,status.unlocked,refresh]);
  useEffect(()=>{
    document.documentElement.dataset.theme=snapshot.settings.theme;
    document.documentElement.style.setProperty('--accent',snapshot.settings.accent);
    document.documentElement.lang=snapshot.settings.language;
  },[snapshot.settings.theme,snapshot.settings.accent,snapshot.settings.language]);
  useEffect(()=>{
    const ids=new Set(snapshot.accounts.map(a=>a.id));
    setSelected(previous=>{const next=previous.filter(id=>ids.has(id));return next.length===previous.length?previous:next;});
  },[snapshot.accounts]);
  useEffect(()=>{
    let last=0;
    const activity=()=>{const now=Date.now();if(now-last>15000){last=now;if(native&&status.unlocked)void api.touch().catch(()=>{});}};
    const shortcuts=(e:KeyboardEvent)=>{
      if((e.ctrlKey||e.metaKey)&&e.key==='k'){e.preventDefault();navigate('accounts');setTimeout(()=>document.querySelector<HTMLInputElement>('.search-input input')?.focus(),50);}
      if((e.ctrlKey||e.metaKey)&&e.key==='Enter'&&status.unlocked){e.preventDefault();void launch();}
      if((e.ctrlKey||e.metaKey)&&e.key==='n'&&status.unlocked){e.preventDefault();setAdd(true);}
    };
    window.addEventListener('pointerdown',activity);window.addEventListener('keydown',activity);window.addEventListener('keydown',shortcuts);
    return()=>{window.removeEventListener('pointerdown',activity);window.removeEventListener('keydown',activity);window.removeEventListener('keydown',shortcuts);};
  },[status.unlocked,navigate,snapshot,selected,target,sameServer,followFirst]);
  useEffect(()=>()=>{if(toastTimer.current)clearTimeout(toastTimer.current);},[]);
  const activeBatch=snapshot.batches.find(b=>['running','queued'].includes(b.state));
  const failedBatch=snapshot.batches.find(b=>b.state==='failed');
  const context={snapshot,status,page,navigate,selected,setSelected,target,setTarget,busy,refresh,notify,run,t,addAccount:()=>setAdd(true),editAccount:setEdit,launch,sameServer,setSameServer,followFirst,setFollowFirst,lock};
  const pages={home:<Home/>,accounts:<Accounts/>,play:<Play/>,servers:<Servers/>,performance:<PerformancePage/>,monitor:<Monitor/>,alerts:<Alerts/>,settings:<SettingsPage key={status.mode}/>,diagnostics:<Diagnostics/>};
  if(!ready)return <div className="boot-screen"><span className="app-mark"><i/></span><strong>AltDeck</strong><RefreshCw className="spin" size={20}/></div>;
  return <AppContext.Provider value={context}>
    {!status.unlocked?<Unlock status={status} onUnlock={refresh}/>:<div className="app-shell" style={snapshot.settings.background?{backgroundImage:`linear-gradient(var(--background-overlay),var(--background-overlay)),url("${snapshot.settings.background}")`}:undefined}>
      {mobileNav&&<div className="sidebar-scrim" onClick={()=>setMobileNav(false)}/>}
      <aside className={`sidebar ${mobileNav?'mobile-open':''}`}><button className="brand" onClick={()=>navigate('home')} aria-label="AltDeck — Overview"><span className="app-mark"><i/></span><span>AltDeck<small>PLAY TOGETHER.</small></span></button>
        <div className="workspace-switch"><span className="workspace-avatar"><Command size={17}/></span><div><strong>Personal workspace</strong><small>{t('localVault')}</small></div><LockKeyhole size={13}/></div>
        <nav aria-label={t('workspace')}><span className="nav-label">{t('workspace')}</span>{mainNav.map(([key,Icon])=><button key={key} className={`nav-item ${page===key?'active':''}`} onClick={()=>navigate(key)}><Icon size={18}/><span>{t(key)}</span>{key==='accounts'&&<em>{snapshot.accounts.length}</em>}{key==='home'&&snapshot.clients.length>0&&<i className="nav-active-dot"/>}</button>)}<span className="nav-label tools-label">{t('tools')}</span>{toolNav.map(([key,Icon])=><button key={key} className={`nav-item ${page===key?'active':''}`} onClick={()=>navigate(key)}><Icon size={18}/><span>{t(key)}</span>{key==='monitor'&&<i className="nav-live-dot"/>}</button>)}</nav>
        <div className="sidebar-bottom"><div className="vault-card"><div><span><ShieldCheck size={17}/></span><strong>{t('localVault')}</strong><i className="live-dot"/></div><p>{native?(status.mode==='password'?t('masterPassword'):t('protected')):t('demoMode')}</p><button onClick={()=>void run(lock)}><LockKeyhole size={12}/>{t('lockNow')}<ArrowUpRight size={12}/></button></div><button className={`nav-item ${page==='settings'?'active':''}`} onClick={()=>navigate('settings')}><SettingsIcon size={18}/><span>{t('settings')}</span></button><button className={`nav-item ${page==='diagnostics'?'active':''}`} onClick={()=>navigate('diagnostics')}><Terminal size={18}/><span>{t('diagnostics')}</span></button><div className="sidebar-version"><span>v0.1.0 <i/> {native?'Windows':'Web preview'}</span><Github size={14}/></div></div>
      </aside>
      <div className="main-shell"><header className="topbar"><div className="breadcrumb"><IconButton className="mobile-menu" label="Menu" onClick={()=>setMobileNav(!mobileNav)}>{mobileNav?<X size={19}/>:<Menu size={19}/>}</IconButton><span>{t('workspace').toLocaleLowerCase()}</span><ChevronRight size={13}/><strong>{t(page)}</strong></div><div className="topbar-actions">{!native&&<span className="demo-pill"><i/>{t('demoMode')}</span>}<span className="local-indicator"><span className="live-dot"/>{native?'Local only':'Preview'}</span><span className="topbar-divider"/><IconButton label={t('refresh')} onClick={()=>void run(refresh)}><RefreshCw size={16}/></IconButton><button className="profile-button" onClick={()=>navigate('settings')} aria-label={t('settings')}>{snapshot.accounts[0]?<Avatar account={snapshot.accounts[0]} size={30}/>:<SettingsIcon size={18}/>}</button></div></header>
        {!native&&<div className="demo-banner"><span><InfoIcon/>{t('demoNotice')}</span><button onClick={()=>navigate('diagnostics')}>{t('learnMore')}<ArrowUpRight size={12}/></button></div>}
        <main className="main-content" ref={content}>
          {activeBatch&&<div className="batch-banner"><RefreshCw size={16} className="spin"/><span>{t('queue')} · {activeBatch.completed} / {activeBatch.total}</span><div className="batch-progress"><i style={{width:`${activeBatch.completed/Math.max(1,activeBatch.total)*100}%`}}/></div><Button className="small" onClick={()=>void run(()=>api.cancelLaunch(activeBatch.id))}>{t('cancel')}</Button></div>}
          {failedBatch&&!activeBatch&&<div className="batch-error"><AlertCircle size={16}/><span>{failedBatch.error}</span><button onClick={()=>navigate('diagnostics')}>{t('diagnostics')}<ArrowUpRight size={13}/></button></div>}
          <div className="page-content" key={page}>{pages[page]}</div>
          <footer className="main-footer"><span>AltDeck <span>·</span> {t('privacy')}</span><span>Made for your alts. Not affiliated with Roblox.<span className="footer-dot"/></span></footer>
        </main>
      </div>
      {add&&<AddAccount onClose={()=>setAdd(false)}/>} {edit&&<EditAccount account={snapshot.accounts.find(a=>a.id===edit.id)||edit} onClose={()=>setEdit(null)}/>}
    </div>}
    {toast&&<div className={`toast ${toast.error?'toast-error':''}`} role={toast.error?'alert':'status'}>{toast.error?<AlertCircle size={19}/>:<Check size={19}/>}<span>{toast.message}</span><IconButton label={t('close')} onClick={()=>setToast(null)}><X size={15}/></IconButton></div>}
  </AppContext.Provider>;
}
function InfoIcon(){return <ShieldCheck size={13}/>;}
