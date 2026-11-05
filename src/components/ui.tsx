import { useEffect, useRef, useState, type ReactNode, type ButtonHTMLAttributes } from 'react';
import { X, LoaderCircle, Check, ArrowUpRight, Box, ChevronRight } from 'lucide-react';
import type { Account } from '../lib/types';
import { useApp } from '../lib/context';

export function Button({children, variant='secondary', loading=false, className='', ...props}: ButtonHTMLAttributes<HTMLButtonElement> & {variant?: 'primary'|'secondary'|'ghost'|'danger'; loading?: boolean}) {
  return <button {...props} disabled={props.disabled || loading} className={`button button-${variant} ${className}`}>{loading && <LoaderCircle size={16} className="spin"/>}{children}</button>;
}
export function IconButton({children, label, ...props}: ButtonHTMLAttributes<HTMLButtonElement> & {label: string}) {
  return <button {...props} className={`icon-button ${props.className || ''}`} aria-label={label} title={label}>{children}</button>;
}
export function Toggle({checked, onChange, label, disabled=false}: {checked:boolean;onChange:(value:boolean)=>void;label:string;disabled?:boolean}) {
  return <button type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} className={`toggle ${checked?'on':''}`} onClick={()=>onChange(!checked)}><span/></button>;
}
export function Field({label, hint, children}: {label:string;hint?:string;children:ReactNode}) { return <label className="field"><span className="field-label">{label}</span>{children}{hint && <span className="field-hint">{hint}</span>}</label>; }
export function Setting({title, description, children}: {title:string;description?:string;children:ReactNode}) { return <div className="setting-row"><div><strong>{title}</strong>{description && <p>{description}</p>}</div><div className="setting-control">{children}</div></div>; }
export function Avatar({account, size=38, status=false}: {account:Pick<Account,'avatarUrl'|'displayName'|'presence'>;size?:number;status?:boolean}) {
  const [failed,setFailed]=useState(false);
  return <span className="avatar" style={{width:size,height:size}}>
    {account.avatarUrl && !failed ? <img src={account.avatarUrl} alt="" onError={()=>setFailed(true)}/> : <span>{account.displayName.slice(0,2).toUpperCase()}</span>}
    {status && <i className={`presence-dot presence-${account.presence}`}/>}
  </span>;
}
export function Badge({children, tone='neutral'}: {children:ReactNode;tone?:string}) { return <span className={`badge badge-${tone}`}><i/>{children}</span>; }
export function SectionHeading({eyebrow,title,description,children}: {eyebrow?:string;title:string;description?:string;children?:ReactNode}) {
  return <div className="page-heading"><div>{eyebrow&&<div className="eyebrow">{eyebrow}</div>}<h1>{title}</h1>{description&&<p>{description}</p>}</div>{children&&<div className="heading-actions">{children}</div>}</div>;
}
export function Empty({title,description,children}: {title:string;description:string;children?:ReactNode}) { return <div className="empty-state"><div className="empty-icon"><Box size={30} strokeWidth={1.3}/></div><h3>{title}</h3><p>{description}</p>{children}</div>; }
export function PanelHeader({title,subtitle,children}: {title:string;subtitle?:string;children?:ReactNode}) { return <div className="panel-header"><div><h2>{title}</h2>{subtitle&&<span>{subtitle}</span>}</div>{children}</div>; }
export function Modal({title,children,onClose,wide=false}: {title:string;children:ReactNode;onClose:()=>void;wide?:boolean}) {
  const ref=useRef<HTMLDivElement>(null);
  const onCloseRef=useRef(onClose); onCloseRef.current=onClose;
  useEffect(()=>{
    const previous=document.activeElement as HTMLElement|null;
    const bodyOverflow=document.body.style.overflow;
    document.body.style.overflow='hidden';
    const node=ref.current;
    const selector='button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]';
    (node?.querySelector(selector) as HTMLElement|null)?.focus();
    const handle=(event:KeyboardEvent)=>{
      if(event.key==='Escape') onCloseRef.current();
      if(event.key==='Tab' && node){
        const items=Array.from(node.querySelectorAll<HTMLElement>(selector));
        const first=items[0], last=items[items.length-1];
        if(event.shiftKey && document.activeElement===first){event.preventDefault();last?.focus();}
        else if(!event.shiftKey && document.activeElement===last){event.preventDefault();first?.focus();}
      }
    };
    document.addEventListener('keydown',handle);
    return()=>{document.removeEventListener('keydown',handle);document.body.style.overflow=bodyOverflow;previous?.focus();};
  },[]);
  return <div className="modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)onClose();}}><div className={`modal ${wide?'modal-wide':''}`} role="dialog" aria-modal="true" aria-label={title} ref={ref}><div className="modal-header"><h2>{title}</h2><IconButton label="Close / Закрыть" onClick={onClose}><X size={19}/></IconButton></div>{children}</div></div>;
}
export function AccountPicker({compact=false}: {compact?:boolean}) {
  const {snapshot,selected,setSelected,t}=useApp();
  return <div className={`account-picker ${compact?'compact':''}`}>
    {snapshot.accounts.map(a=><button key={a.id} className={`account-chip ${selected.includes(a.id)?'chosen':''}`} onClick={()=>setSelected(selected.includes(a.id)?selected.filter(id=>id!==a.id):[...selected,a.id])} aria-pressed={selected.includes(a.id)}>
      <Avatar account={a} size={compact?28:34}/><span>{a.alias||a.displayName}</span><span className="chip-check">{selected.includes(a.id)&&<Check size={12}/>}</span>
    </button>)}
    {!snapshot.accounts.length&&<button className="text-button" onClick={()=>document.getElementById('add-account-button')?.click()}>{t('addAccount')} <ArrowUpRight size={15}/></button>}
  </div>;
}
export function TextLink({children,onClick}: {children:ReactNode;onClick:()=>void}) { return <button className="text-link" onClick={onClick}>{children}<ChevronRight size={15}/></button>; }
export function downloadJson(name:string, value:unknown) {
  const url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:'application/json'}));
  const a=document.createElement('a'); a.href=url; a.download=name; a.click(); setTimeout(()=>URL.revokeObjectURL(url),1000);
}
export function formatBytes(mb:number) { return mb>=1024?`${(mb/1024).toFixed(2)} GB`:`${Math.round(mb)} MB`; }
export function uptime(start:number) { const sec=Math.max(0,Math.floor(Date.now()/1000-start));return `${Math.floor(sec/3600).toString().padStart(2,'0')}:${Math.floor(sec%3600/60).toString().padStart(2,'0')}:${(sec%60).toString().padStart(2,'0')}`; }
