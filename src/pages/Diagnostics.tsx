import { useEffect, useState } from 'react';
import { Copy, ShieldCheck, Terminal, RefreshCw, Github, FileText } from 'lucide-react';
import { useApp } from '../lib/context';
import { api, native } from '../lib/api';
import { Button, Empty, SectionHeading, downloadJson } from '../components/ui';
import type { DiagnosticEvent } from '../lib/types';
export function Diagnostics() {
  const {t,run,status}=useApp(); const [events,setEvents]=useState<DiagnosticEvent[]>([]);
  const load=()=>void run(async()=>setEvents(await api.diagnostics()));
  useEffect(()=>{load();},[]);
  return <><SectionHeading eyebrow={t('diagnostics')} title={t('diagTitle')} description={t('diagSubtitle')}><Button onClick={load}><RefreshCw size={15}/>{t('refresh')}</Button><Button onClick={()=>void run(async()=>{await navigator.clipboard.writeText(JSON.stringify(events,null,2));},t('copied'))}><Copy size={15}/>{t('copyLogs')}</Button></SectionHeading><div className="diagnostic-meta"><span><ShieldCheck size={18}/>No cookies / launch tickets</span><span>AltDeck {status.version}</span><span>{native?'Tauri 2 · Rust':'Browser demo'}</span></div><section className="panel diagnostic-console"><div className="panel-header"><h2><Terminal size={17}/>Event log</h2><Button className="small" onClick={()=>downloadJson('altdeck-diagnostics.json',events)}><FileText size={14}/>JSON</Button></div>{events.length?events.map((e,i)=><div className="log-row" key={i}><time>{new Date(e.at*1000).toLocaleTimeString()}</time><span>{e.code}</span><p>{e.message}</p></div>):<Empty title={t('noLogs')} description={t('privacyText')}/>}</section><div className="information-box"><Github size={20}/><p>AltDeck is an independent project, not affiliated with Roblox or RoALT. See README.md and docs/STATUS.md in the source for implemented features, limitations and the Windows test checklist.</p></div></>;
}
