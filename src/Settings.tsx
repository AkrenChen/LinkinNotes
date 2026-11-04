import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type Priority = 'ui' | 'nui' | 'uni' | 'nuni';
export type Preferences = { shortcut: string; names: Record<Priority, string> };
export const defaults: Preferences = { shortcut: 'Ctrl+Alt+Q', names: { ui: '重要且紧急', nui: '重要不紧急', uni: '紧急不重要', nuni: '不重要不紧急' } };

export function SettingsPanel({ preferences, onClose, onSaved }: { preferences: Preferences; onClose: () => void; onSaved: (next: Preferences) => void }) {
  const [settings, setSettings] = useState<Preferences>({ ...preferences, names: { ...preferences.names } });
  const [recording, setRecording] = useState(false), [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const panel = useRef<HTMLElement>(null), recorder = useRef<HTMLButtonElement>(null);
  useEffect(() => { const previous = document.activeElement as HTMLElement; panel.current?.querySelector<HTMLElement>('button')?.focus(); return () => previous?.focus(); }, []);
  async function save() {
    setBusy(true); setError('');
    try { onSaved(await invoke<Preferences>('save_settings', { settings })); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  return <div className="settings-dismiss" onMouseDown={e => { if (e.target === e.currentTarget && !busy) onClose(); }}>
    <aside className="settings-panel" role="dialog" aria-modal="true" aria-labelledby="settings-title" ref={panel} onKeyDown={e => {
      if (e.key === 'Escape' && !busy) { e.stopPropagation(); recording ? setRecording(false) : onClose(); }
      if (e.key === 'Tab') { const items = Array.from(panel.current!.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)')); const first = items[0], last = items[items.length - 1]; if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); } else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); } }
    }}>
      <header><h2 id="settings-title">设置</h2><button className="icon-button" aria-label="关闭设置" disabled={busy} onClick={onClose}>×</button></header>
      <label>显示 / 隐藏快捷键</label>
      <button ref={recorder} className={`shortcut-recorder ${recording ? 'recording' : ''}`} disabled={busy} onClick={() => { setRecording(true); recorder.current?.focus(); }} onBlur={() => setRecording(false)} onKeyDown={e => {
        if (!recording) return;
        e.preventDefault(); e.stopPropagation();
        if (e.key === 'Escape') { setRecording(false); return; }
        if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return;
        const key = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : /^Digit[0-9]$/.test(e.code) ? e.code.slice(5) : /^F([1-9]|1[0-9]|2[0-4])$/.test(e.code) ? e.code : null;
        if (!key || (!e.ctrlKey && !e.altKey && !e.shiftKey) || e.metaKey) { setError('请使用 Ctrl / Alt / Shift 搭配字母、数字或功能键。'); return; }
        setSettings({ ...settings, shortcut: [...(e.ctrlKey ? ['Ctrl'] : []), ...(e.altKey ? ['Alt'] : []), ...(e.shiftKey ? ['Shift'] : []), key].join('+') });
        setRecording(false); setError('');
      }}>{recording ? '请按下新的组合键…' : settings.shortcut || '未设置 · 点击录入'}</button>
      <p className="setting-help">窗口在前台时按下隐藏；隐藏、最小化或不在前台时按下唤起。</p>
      <div className="setting-inline-actions"><button disabled={busy} onClick={() => setSettings({ ...settings, shortcut: defaults.shortcut })}>恢复默认</button><button disabled={busy} onClick={() => setSettings({ ...settings, shortcut: '' })}>不使用快捷键</button></div>
      <fieldset disabled={busy}><legend>象限名称</legend>{(Object.keys(defaults.names) as Priority[]).map((key, index) => <label className={`name-setting ${key}`} key={key}><span>0{index + 1}</span><input aria-label={`第${index + 1}象限名称`} value={settings.names[key]} maxLength={24} onChange={e => setSettings({ ...settings, names: { ...settings.names, [key]: e.target.value } })}/></label>)}</fieldset>
      <p className="setting-help">名称只影响浮窗显示，任务仍同步到对应象限。</p>
      {error && <p className="settings-error" role="alert">{error}</p>}
      <div className="settings-actions"><button className="secondary" disabled={busy} onClick={onClose}>取消</button><button className="primary" disabled={busy || recording} onClick={() => void save()}>{busy ? '保存中…' : '保存设置'}</button></div>
    </aside>
  </div>;
}
