import { useState } from 'react';
import { Star, ArrowUpRight, Trash2, BookmarkPlus, Clock3 } from 'lucide-react';
import { useApp } from '../lib/context';
import { api } from '../lib/api';
import { QuickLaunch } from './Home';
import { Button, IconButton, SectionHeading, Empty } from '../components/ui';
export function Play() {
  const {snapshot,t,target,setTarget,run,refresh,navigate}=useApp(); const [name,setName]=useState('');
  return <><SectionHeading eyebrow={t('play')} title={t('playTitle')} description={t('playSubtitle')}/><div className="play-grid"><QuickLaunch full/><div className="play-side"><section className="panel favorites-panel"><div className="panel-header"><h2>{t('savedGames')}</h2><Star size={17}/></div>{snapshot.favorites.length?snapshot.favorites.map((f,i)=><div className="favorite-row" key={f.id}><button onClick={()=>setTarget(f.target)}><span className={`favorite-art game-color-${i%3}`}>{f.icon?<img src={f.icon} alt=""/>:f.name.slice(0,1)}</span><span><strong>{f.name}</strong><small>{f.target.length>35?f.target.slice(0,35)+'…':f.target}</small></span><ArrowUpRight size={16}/></button><IconButton label={t('remove')} onClick={()=>void run(async()=>{await api.removeFavorite(f.id);await refresh();})}><Trash2 size={14}/></IconButton></div>):<Empty title={t('empty')} description={t('saveDestination')}/>}
      <div className="favorite-add"><input value={name} onChange={e=>setName(e.target.value)} placeholder={t('alias')} aria-label={t('alias')}/><Button disabled={!target||!name} onClick={()=>void run(async()=>{await api.saveFavorite(name,target);setName('');await refresh();},t('saved'))}><BookmarkPlus size={16}/>{t('save')}</Button></div></section>
      <section className="panel history-panel"><div className="panel-header"><h2>{snapshot.settings.language==='ru'?'История запусков':'Launch history'}</h2><Clock3 size={16}/></div>{snapshot.history.length?snapshot.history.slice(0,6).map((h,i)=>{const a=snapshot.accounts.find(a=>a.id===h.accountId);return <div className="history-row" key={i}><span className="live-dot"/><div><strong>{a?.displayName||'Account'}</strong><small>{h.target.kind==='follow'?`User ${h.target.userId}`:`Place ${h.target.placeId}`}</small></div><time>{new Date(h.at*1000).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})}</time></div>;}):<p className="panel-placeholder">{t('empty')}</p>}</section>
      <Button className="wide" onClick={()=>navigate('servers')}>{t('servers')}<ArrowUpRight size={16}/></Button></div></div></>;
}
