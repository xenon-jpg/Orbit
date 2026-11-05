import { useState } from 'react';
import { ArrowUpRight, ArrowRight, Users, Monitor, ShieldCheck, MemoryStick, LayoutGrid, Gamepad2, Plus, Sparkles, ChevronDown, LockKeyhole } from 'lucide-react';
import { useApp } from '../lib/context';
import { Avatar, Button, PanelHeader, SectionHeading, Empty, Toggle, TextLink } from '../components/ui';
import { ClientCard } from '../components/ClientCard';
import { StopAll } from '../components/StopAll';
import { native, api } from '../lib/api';

export function QuickLaunch({full=false}: {full?:boolean}) {
  const {t,snapshot,target,setTarget,selected,setSelected,sameServer,setSameServer,followFirst,setFollowFirst,launch,busy,run,refresh}=useApp();
  const [expanded,setExpanded]=useState(full);
  return <section className={`panel quick-launch ${full?'full-launch':''}`}>
    <PanelHeader title={t(full?'ready':'quickLaunch')}><Gamepad2 size={19}/></PanelHeader>
    <div className="quick-launch-body">
      <label className="field"><span className="field-label"><span className="step-number">1</span>{t('destination')}</span><div className="input-with-icon"><Gamepad2 size={17}/><input value={target} onChange={e=>setTarget(e.target.value)} placeholder={t('gamePlaceholder')} aria-label={t('destination')} onKeyDown={e=>{if(e.key==='Enter')void launch();}}/></div></label>
      <div className="saved-inline">{snapshot.favorites.slice(0,3).map((f,i)=><button key={f.id} onClick={()=>setTarget(f.target)} className={target===f.target?'active':''}><span className={`mini-game game-color-${i}`}>{f.name.slice(0,1)}</span>{f.name}</button>)}</div>
      <div className="field"><div className="field-label"><span><span className="step-number">2</span>{t('whoPlays')}</span><button className="muted-button" onClick={()=>setSelected(selected.length===snapshot.accounts.length?[]:snapshot.accounts.map(a=>a.id))}>{selected.length===snapshot.accounts.length?t('clear'):t('selectAll')}</button></div>
        <div className={`account-picker ${full?'launch-account-grid':'compact'}`}>{snapshot.accounts.map(a=><button key={a.id} className={`account-chip ${selected.includes(a.id)?'chosen':''}`} onClick={()=>setSelected(selected.includes(a.id)?selected.filter(id=>id!==a.id):[...selected,a.id])} aria-pressed={selected.includes(a.id)}><Avatar account={a} size={full?36:28}/><span>{a.alias||a.displayName}{full&&<small>@{a.username}</small>}</span><span className="chip-check">{selected.includes(a.id)&&'✓'}</span></button>)}</div>
      </div>
      <div className="same-server-row"><span><Users size={15}/>{t('sameServer')}</span><Toggle checked={sameServer} onChange={setSameServer} label={t('sameServer')}/></div>
      {(expanded||full)&&<div className="launch-extra"><div className="same-server-row"><span>{t('followFirst')}</span><Toggle checked={followFirst} onChange={setFollowFirst} label={t('followFirst')}/></div><p className="field-hint">{t('followHelp')}</p><div className="inline-note"><LockKeyhole size={14}/>{t('delay')}: {snapshot.settings.launchDelaySeconds} {t('seconds')}</div></div>}
      <Button variant="primary" className="launch-button" loading={busy} disabled={!target||!selected.length} onClick={()=>void launch()}><Gamepad2 size={18}/>{t('launchSelected')}<span>{selected.length}</span><ArrowRight size={17}/></Button>
      {!full&&<button className="more-options" onClick={()=>setExpanded(!expanded)}>{t('moreOptions')}<ChevronDown size={13} className={expanded?'rotate':''}/></button>}
      {full&&<><Button className="wide" disabled={!target} onClick={()=>void run(async()=>{await api.saveFavorite(target.slice(0,40),target);await refresh();},t('saved'))}><Plus size={15}/>{t('saveDestination')}</Button><p className="launch-privacy"><ShieldCheck size={13}/>{t('privacyText')}</p></>}
    </div>
  </section>;
}
export function Stats() {
  const {snapshot,t}=useApp(); const active=snapshot.clients.filter(c=>!['stopped','failed'].includes(c.state));
  const valid=snapshot.accounts.filter(a=>a.sessionStatus==='valid').length;
  const stats=[{label:t('running'),value:active.length,unit:`/ ${snapshot.settings.maxClients}`,icon:Monitor,tone:'green',detail:t('connected')},{label:t('totalAccounts'),value:snapshot.accounts.length,unit:'',icon:Users,tone:'blue',detail:native?t('localVault'):t('demoMode')},{label:t('validSessions'),value:valid,unit:`/ ${snapshot.accounts.length}`,icon:ShieldCheck,tone:'purple',detail:native?t('protected'):t('demoMode')},{label:t('memory'),value:(active.reduce((n,c)=>n+c.memoryMb,0)/1024).toFixed(1),unit:'GB',icon:MemoryStick,tone:'orange',detail:'AltDeck clients'}];
  return <div className="stats-grid">{stats.map(s=><div className="stat-card" key={s.label}><div className="stat-label"><span>{s.label}</span><span className={`stat-icon ${s.tone}`}><s.icon size={17}/></span></div><div className="stat-value">{s.value}<span>{s.unit}</span></div><div className="stat-foot">{s.tone==='green'?<i className="live-dot"/>:<s.icon size={11}/>} {s.detail}</div></div>)}</div>;
}
export function Home() {
  const {snapshot,t,navigate,addAccount,run,notify}=useApp();
  const [accountTab,setAccountTab]=useState('all');
  const clients=snapshot.clients.filter(c=>accountTab==='all'||c.accountId===accountTab);
  return <>
    <SectionHeading eyebrow={t('overview')} title={t('greeting')} description={t('subtitle')}><Button onClick={addAccount}><Plus size={16}/>{t('addAccount')}</Button></SectionHeading>
    <Stats/>
    <div className="home-grid"><section className="clients-section"><PanelHeader title={t('clients')} subtitle={`${clients.length} ${t('running').toLowerCase()}`}><div className="panel-actions"><Button className="small tile-button" onClick={()=>{if(native)void run(()=>api.tile());else notify(t('demoAction'));}}><LayoutGrid size={14}/>{t('tile')}</Button><StopAll/></div></PanelHeader>
      <div className="client-tabs"><button className={accountTab==='all'?'active':''} onClick={()=>setAccountTab('all')}><LayoutGrid size={13}/>{t('all')}</button>{snapshot.clients.slice(0,4).map(c=>{const a=snapshot.accounts.find(a=>a.id===c.accountId);return a&&<button key={c.id} className={accountTab===c.accountId?'active':''} onClick={()=>setAccountTab(c.accountId)}><Avatar account={a} size={19}/>{a.alias||a.displayName}</button>;})}</div>
      {clients.length?<div className="clients-grid">{clients.map((c,i)=><ClientCard key={c.id} client={c} index={i}/>)}</div>:<div className="panel"><Empty title={t('noClients')} description={t('noClientsText')}><Button variant="primary" onClick={()=>navigate('play')}>{t('firstLaunch')}<ArrowUpRight size={16}/></Button></Empty></div>}
      <div className="dashboard-bottom-note"><span><span className="live-dot"/>{native?t('previewHint'):'Demo scene · not a Roblox screenshot'}</span><TextLink onClick={()=>navigate('monitor')}>{t('monitor')}</TextLink></div>
      <div className="privacy-strip"><span className="privacy-icon"><ShieldCheck size={23}/></span><div><strong>{t('privacy')}</strong><p>{t('privacyText')}</p></div><span className="privacy-tag">NO TELEMETRY<ArrowUpRight size={12}/></span></div>
    </section><aside className="home-right"><QuickLaunch/><div className="tip-card"><Sparkles size={18}/><div><strong>{t('performanceTitle')}</strong><p>{t('performanceSubtitle')}</p><TextLink onClick={()=>navigate('performance')}>{t('performance')}</TextLink></div></div></aside></div>
  </>;
}
