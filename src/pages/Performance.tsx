import { useRef, useState } from 'react';
import { Gauge, Leaf, Layers3, Download, Upload, Save, Cpu, Info, Check } from 'lucide-react';
import { useApp } from '../lib/context';
import { api, native } from '../lib/api';
import { defaultPerformance } from '../lib/demo';
import { Button, SectionHeading, Setting, Toggle, downloadJson } from '../components/ui';
import type { Performance } from '../lib/types';
export function validateProfile(value: unknown): Performance {
  if (!value || typeof value !== 'object') throw new Error('Invalid profile');
  const v=value as Record<string,unknown>;
  if (typeof v.fps!=='number'||!Number.isFinite(v.fps)||v.fps<10||v.fps>240) throw new Error('FPS must be 10–240');
  const profile={...defaultPerformance};
  profile.fps=Math.round(v.fps); profile.preset=typeof v.preset==='string'?v.preset.slice(0,30):'custom';
  for(const key of ['disableGrass','graySky','disableShadows','fairCpu','trimMemory','tinyWindows'] as const) if(typeof v[key]==='boolean')profile[key]=v[key];
  for(const key of ['textureQuality','cpuPercent'] as const) if(typeof v[key]==='number'&&Number.isFinite(v[key]))profile[key]=Math.round(Math.max(0,Math.min(key==='textureQuality'?3:100,v[key])));
  return profile;
}
export function PerformancePage() {
  const {snapshot,t,run,refresh,notify}=useApp(); const [profile,setProfile]=useState<Performance>({...snapshot.settings.performance}); const file=useRef<HTMLInputElement>(null);
  const update=<K extends keyof Performance>(key:K,value:Performance[K])=>setProfile(p=>({...p,[key]:value,preset:'custom'}));
  const preset=(name:string)=>{const p={...defaultPerformance,preset:name};if(name==='quality'){p.fps=60;p.textureQuality=3;}if(name==='maxAlts'){p.fps=10;p.textureQuality=1;p.disableGrass=true;p.disableShadows=true;p.graySky=true;p.fairCpu=true;p.trimMemory=true;p.tinyWindows=true;}setProfile(p);};
  return <><SectionHeading eyebrow={t('performance')} title={t('performanceTitle')} description={t('performanceSubtitle')}><Button onClick={()=>downloadJson('altdeck-performance.json',profile)}><Download size={15}/>{t('exportProfile')}</Button><Button onClick={()=>file.current?.click()}><Upload size={15}/>{t('importProfile')}</Button><input ref={file} type="file" accept=".json,application/json" hidden onChange={async e=>{const f=e.target.files?.[0];if(!f)return;try{if(f.size>16384)throw new Error('Profile too large');setProfile(validateProfile(JSON.parse(await f.text())));notify(t('importProfile'));}catch{notify('Invalid profile / Некорректный профиль',true);}e.target.value='';}}/></SectionHeading>
    <div className="preset-grid">{[{id:'balanced',icon:Gauge,color:'green'},{id:'quality',icon:Layers3,color:'purple'},{id:'maxAlts',icon:Leaf,color:'blue'}].map(p=><button key={p.id} className={`preset-card ${profile.preset===p.id?'active':''}`} onClick={()=>preset(p.id)}><span className={`stat-icon ${p.color}`}><p.icon size={23}/></span><div><strong>{t(p.id as 'balanced'|'quality'|'maxAlts')}</strong><span>{p.id==='balanced'?'30 FPS · Balanced rendering':p.id==='quality'?'60 FPS · Full textures':'10 FPS · Shared CPU'}</span></div><span className="preset-radio">{profile.preset===p.id&&<Check size={12}/>}</span></button>)}</div>
    <div className="settings-columns"><section className="panel settings-panel"><div className="panel-header"><h2><Gauge size={18}/>{t('performance')}</h2></div><div className="fps-control"><div><strong>{t('fpsLimit')}</strong><span>{profile.fps}<small>FPS</small></span></div><input type="range" min="10" max="240" step="5" value={profile.fps} onChange={e=>update('fps',Number(e.target.value))} aria-label={t('fpsLimit')}/><div className="range-labels"><span>10 FPS</span><span>60 FPS</span><span>240 FPS</span></div></div>
      <Setting title={t('textures')}><select value={profile.textureQuality} onChange={e=>update('textureQuality',Number(e.target.value))}>{['default','low','medium','high'].map((key,i)=><option value={i} key={key}>{t(key as 'default'|'low'|'medium'|'high')}</option>)}</select></Setting>
      {([['disableGrass','noGrass'],['graySky','graySky'],['disableShadows','lowShadows'],['tinyWindows','tinyWindows']] as const).map(([key,label])=><Setting key={key} title={t(label)}><Toggle checked={profile[key]} onChange={v=>update(key,v)} label={t(label)}/></Setting>)}
    </section><section className="panel settings-panel"><div className="panel-header"><h2><Cpu size={18}/>CPU & {t('memory')}</h2></div><Setting title={t('fairCpu')} description="80% CPU / active clients"><Toggle checked={profile.fairCpu} onChange={v=>update('fairCpu',v)} label={t('fairCpu')}/></Setting><Setting title={t('cpuLimit')}><select disabled={profile.fairCpu} value={profile.cpuPercent} onChange={e=>update('cpuPercent',Number(e.target.value))}><option value={0}>{t('unlimited')}</option>{[10,20,30,50,75].map(n=><option key={n} value={n}>{n}%</option>)}</select></Setting><Setting title={t('trimMemory')} description="Working set > 1.8 GB"><Toggle checked={profile.trimMemory} onChange={v=>update('trimMemory',v)} label={t('trimMemory')}/></Setting><div className="information-box"><Info size={18}/><p>{t('profileHint')}</p></div></section></div>
    <div className="save-profile-bar"><span><Info size={15}/>{native?t('profileHint'):t('demoNotice')}</span><Button variant="primary" onClick={()=>void run(async()=>{await api.saveSettings({...snapshot.settings,performance:profile});if(native)await api.applyPerformance(profile);await refresh();},t('saved'))}><Save size={16}/>{t('applyProfile')}</Button></div>
  </>;
}
