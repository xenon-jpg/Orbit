import { useEffect, useRef, useState } from 'react';
import { Square } from 'lucide-react';
import { useApp } from '../lib/context';
import { api, native } from '../lib/api';
export function StopAll() {
  const {t,run,refresh,snapshot}=useApp();
  const [progress,setProgress]=useState(0);
  const timer=useRef<ReturnType<typeof setInterval>|null>(null);
  const fired=useRef(false);
  const stop=()=>{if(timer.current)clearInterval(timer.current);timer.current=null;setProgress(0);};
  useEffect(()=>()=>{if(timer.current)clearInterval(timer.current);},[]);
  const execute=async()=>{if(fired.current)return;fired.current=true;stop();await run(()=>api.stopAll(),!native?t('demoAction'):t('stopped'));await refresh();};
  const start=()=>{if(!snapshot.clients.length)return;fired.current=false;timer.current=setInterval(()=>setProgress(p=>{if(p>=100){void execute();return 100;}return p+5;}),60);};
  return <button className="button stop-button" disabled={!snapshot.clients.length} onPointerDown={start} onPointerUp={stop} onPointerLeave={stop} onPointerCancel={stop} onKeyDown={e=>{if((e.key===' '||e.key==='Enter')&&!e.repeat){e.preventDefault();start();}}} onKeyUp={stop} onBlur={stop} title={t('holdStop')} aria-label={t('holdStop')}><span className="stop-progress" style={{width:`${progress}%`}}/><Square size={12}/>{t('stopAll')}</button>;
}
