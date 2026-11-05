import { useEffect, useRef, useState } from 'react';
import { Maximize2, Minimize2, MoreHorizontal, X, Clock3, Cpu, MemoryStick, Expand, Focus, Trash2 } from 'lucide-react';
import { useApp } from '../lib/context';
import { api, native } from '../lib/api';
import { Avatar, Badge, IconButton, formatBytes, uptime } from './ui';
import type { Client } from '../lib/types';

export function GameScene({index=0}: {index?:number}) {
  const colors=[['#95c9db','#b4d894','#699765'],['#aec6df','#dcc6ad','#7aa49b'],['#b7b6d9','#b8cfa2','#789767']][index%3];
  return <svg className="game-scene" viewBox="0 0 640 350" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
    <defs><linearGradient id={`sky-${index}`} x2="0" y2="1"><stop stopColor={colors[0]}/><stop offset="1" stopColor="#e4ece1"/></linearGradient><linearGradient id={`ground-${index}`} x2="0" y2="1"><stop stopColor={colors[1]}/><stop offset="1" stopColor={colors[2]}/></linearGradient></defs>
    <rect width="640" height="350" fill={`url(#sky-${index})`}/>
    <g fill="#fff" opacity=".65"><path d="M52 66h90v14H38V72h14z"/><path d="M415 48h91v15H397V55h18z"/><path d="M278 88h69v12h-82v-7h13z"/></g>
    <path d="M0 192l85-47 71 22 79-46 72 40 65-19 70 26 103-53 95 50v185H0" fill="#90afa0" opacity=".65"/>
    <path d="M0 213l128-27 126 17 111-19 125 21 150-26v171H0" fill={`url(#ground-${index})`}/>
    <path d="M298 197h49l125 153H152z" fill="#e5d6b9"/><path d="M311 197h17l26 153h-76z" fill="#eee3cf" opacity=".65"/>
    <g><path d="M97 157l97-57 93 57v83H97z" fill="#ead9c5"/><path d="M82 158l112-72 109 72-13 12-96-58-97 59z" fill={index%2?'#827789':'#aa7666'}/><path d="M194 112l93 58v70l-93-10z" fill="#ccbda9"/><rect x="170" y="179" width="37" height="54" rx="2" fill="#8a9b98"/><rect x="118" y="174" width="33" height="32" fill="#adcbd0"/><path d="M134 174v32m-16-16h33" stroke="#f7eee0" strokeWidth="4"/><rect x="229" y="174" width="33" height="32" fill="#adcbd0"/><path d="M245 174v32m-16-16h33" stroke="#f7eee0" strokeWidth="4"/><rect x="180" y="102" width="18" height="26" fill="#bca18c"/></g>
    <g><path d="M412 166l77-43 80 43v67H412z" fill="#e6e5d3"/><path d="M398 168l91-60 94 60-13 11-81-46-80 46z" fill="#7c9caa"/><rect x="473" y="184" width="29" height="48" fill="#8a9e9a"/><rect x="426" y="181" width="28" height="27" fill="#b7d6d9"/><rect x="522" y="181" width="28" height="27" fill="#b7d6d9"/></g>
    <g><path d="M52 215v-62" stroke="#867b5b" strokeWidth="11"/><path d="M19 165l35-66 39 66z" fill="#659879"/><path d="M24 143l30-57 33 57z" fill="#75a384"/><path d="M595 232v-73" stroke="#867b5b" strokeWidth="12"/><path d="M550 174l46-85 46 85z" fill="#5f9274"/><path d="M558 145l38-69 37 69z" fill="#73a281"/></g>
    <g fill="#f1e8cf"><path d="M35 266h105v8H35zm0 20h105v8H35z"/><path d="M41 259v49m42-49v49m44-49v49" stroke="#f1e8cf" strokeWidth="7"/></g>
    <g transform={`translate(${310+index*13} 238)`}><ellipse cx="0" cy="57" rx="23" ry="7" fill="#506653" opacity=".3"/><path d="M-14 26h12v29h-12zm17 0h12v29H3" fill="#48566a"/><path d="M-18-2h36v34h-36z" fill={['#c4df88','#afa3db','#e9bd86'][index%3]}/><path d="M-28 0h10v29h-10zm46 0h10v29H18" fill="#e5bb91"/><rect x="-13" y="-28" width="26" height="26" rx="5" fill="#e5bb91"/><path d="M-15-25v-9h28v10H-8v8h-7" fill="#655044"/><circle cx="-5" cy="-15" r="1.6" fill="#463c36"/><circle cx="6" cy="-15" r="1.6" fill="#463c36"/><path d="M-3-8h7" stroke="#8f6650" strokeWidth="2"/></g>
    <g fill="#ecda9e"><circle cx="442" cy="281" r="3"/><circle cx="469" cy="265" r="3"/><circle cx="174" cy="287" r="3"/><circle cx="196" cy="302" r="3"/></g>
  </svg>;
}
export function ClientCard({client,index}: {client:Client;index:number}) {
  const {snapshot,t,run,notify}=useApp();
  const account=snapshot.accounts.find(a=>a.id===client.accountId);
  const [menu,setMenu]=useState(false);
  const [preview,setPreview]=useState('');
  const menuRef=useRef<HTMLDivElement>(null);
  useEffect(()=>{
    if(!native || client.minimized)return;
    let active=true, timer:ReturnType<typeof setTimeout>;
    const capture=async()=>{try{const image=await api.preview(client.id);if(active)setPreview(image);}catch{/* Keep a neutral placeholder, never a fabricated screenshot. */}if(active)timer=setTimeout(capture,1200);};
    void capture();return()=>{active=false;clearTimeout(timer);};
  },[client.id,client.minimized]);
  useEffect(()=>{if(!menu)return;const handler=(e:MouseEvent)=>{if(!menuRef.current?.contains(e.target as Node))setMenu(false);};document.addEventListener('mousedown',handler);return()=>document.removeEventListener('mousedown',handler);},[menu]);
  const action=(value:string)=>{setMenu(false);void run(()=>api.clientAction(client.id,value),!native?t('demoAction'):undefined);};
  if(!account)return null;
  const stateKey=(['running','starting','waiting','stopping','stopped','failed','disconnected'].includes(client.state)?client.state:'waiting') as 'running'|'starting'|'waiting'|'stopping'|'stopped'|'failed'|'disconnected';
  return <article className="client-card">
    <div className="client-card-top"><div className="client-account"><Avatar account={account} size={31}/><div><strong>{account.alias||account.displayName}</strong><span>@{account.username}</span></div></div><div className="client-top-actions"><Badge tone={client.state==='running'?'green':client.state==='failed'?'red':'neutral'}>{t(stateKey)}</Badge><div className="menu-anchor" ref={menuRef}><IconButton label={t('actions')} onClick={()=>setMenu(!menu)}><MoreHorizontal size={18}/></IconButton>{menu&&<div className="dropdown"><button onClick={()=>action('focus')}><Focus size={15}/>{t('focus')}</button><button onClick={()=>action('minimize')}><Minimize2 size={15}/>{t('minimize')}</button><button onClick={()=>action('small')}><Expand size={15}/>{t('shrink')}</button><button onClick={()=>action('trim')}><MemoryStick size={15}/>{t('trim')}</button><button className="danger-text" onClick={()=>action('force-close')}><Trash2 size={15}/>{t('forceClose')}</button></div>}</div></div></div>
    <div className={`client-preview ${client.minimized?'is-minimized':''}`}>
      {native?(preview?<img src={preview} alt={`Roblox — ${account.displayName}`}/>:<div className="native-preview-placeholder"><Focus size={30}/><span>{t(client.windowFound?'waiting':'starting')}</span></div>):<GameScene index={index}/>}
      <span className="preview-game-label">{native?(client.windowFound?'Windows preview':'Waiting for window'):'DEMO PREVIEW'}</span>
      <div className="preview-controls"><IconButton label={t('focus')} onClick={()=>{if(native)action('focus');else notify(t('demoAction'));}}><Maximize2 size={16}/></IconButton><IconButton label={t('close')} onClick={()=>action('close')}><X size={16}/></IconButton></div>
      {client.minimized&&<div className="minimized-overlay"><Minimize2 size={22}/><span>{t('minimized')}</span></div>}
    </div>
    <div className="client-game"><span className="game-small-icon">{(client.gameName||'R').slice(0,1)}</span><strong>{client.gameName||t('game')}</strong><span className="client-pid">PID {client.pid}</span></div>
    <div className="client-stats"><span title="CPU"><Cpu size={13}/>{client.cpu.toFixed(1)}<small>%</small></span><span title={t('memory')}><MemoryStick size={13}/>{formatBytes(client.memoryMb)}</span><span className="uptime" title={t('uptime')}><Clock3 size={12}/>{uptime(client.startedAt)}</span></div>
  </article>;
}
