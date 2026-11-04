import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { SettingsPanel, defaults, type Preferences, type Priority } from './Settings';

type Task = { id: string; text: string; priority: Priority; update_at: number; revision?: string; editable: boolean; recurring: boolean };
type Snapshot = { tasks: Task[]; pending: number; failed: number; account?: string };
type Draft = { task?: Task; text: string; priority: Priority; account?: string };
type Context = { x: number; y: number; task?: Task; account?: string; editing?: boolean };
const priorities: Priority[] = ['ui', 'nui', 'uni', 'nuni'];
const message = (e: unknown) => typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);

function Icon({ name }: { name: 'pin' | 'refresh' | 'hide' | 'plus' | 'maximize' | 'restore' | 'menu' | 'check' | 'close' | 'trash' }) {
  return <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {name === 'pin' && <><path d="M8 3h8l-1 7 3 3v2H6v-2l3-3z"/><path d="M12 15v6"/></>}
    {name === 'refresh' && <><path d="M20 10a8 8 0 1 0-2 8M20 4v6h-6"/></>}
    {name === 'hide' && <path d="M5 12h14"/>}{name === 'plus' && <path d="M12 5v14M5 12h14"/>}
    {name === 'maximize' && <rect x="5" y="5" width="14" height="14" rx="1"/>}
    {name === 'restore' && <><path d="M9 5V3h12v12h-2"/><rect x="3" y="9" width="12" height="12" rx="1"/></>}
    {name === 'menu' && <path d="M4 6h16M4 12h16M4 18h16"/>}
    {name === 'check' && <path d="m5 12 4 4L19 6"/>}
    {name === 'close' && <path d="m6 6 12 12M18 6 6 18"/>}
    {name === 'trash' && <><path d="M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7"/></>}
  </svg>;
}

function InlineEditor({ draft, busy, onChange, onSave, onCancel, onContext }: { draft: Draft; busy: boolean; onChange: (text: string) => void; onSave: () => void; onCancel: () => void; onContext: (e: React.MouseEvent) => void }) {
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => { const input = field.current; if (input) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); } }, []);
  useEffect(() => { if (field.current) { field.current.style.height = '0'; field.current.style.height = `${Math.max(24, field.current.scrollHeight)}px`; } }, [draft.text]);
  return <div className="inline-editor" onContextMenu={onContext}>
    <button className="complete-button editor-checkbox" aria-label="编辑中，请先保存或取消再完成任务" title="先保存或取消编辑，再勾选完成" disabled><span>✓</span></button>
    <textarea ref={field} rows={1} aria-label={draft.task ? '编辑任务内容' : '新任务内容'} value={draft.text} maxLength={10000} disabled={busy} placeholder="输入任务，Enter 保存" onChange={e => onChange(e.target.value)} onKeyDown={e => {
      if (e.nativeEvent.isComposing || e.keyCode === 229) return;
      if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onSave(); }
      if (e.key === 'Escape') { e.preventDefault(); if (!busy) onCancel(); }
    }}/>
    <div className="editor-actions"><button className="row-action save-action" aria-label="保存任务" title="保存 · Enter" disabled={busy || !draft.text.trim()} onClick={onSave}><Icon name="check"/></button>
    <button className="row-action" aria-label="取消编辑" title="取消 · Esc" disabled={busy} onClick={onCancel}><Icon name="close"/></button></div>
  </div>;
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>({ tasks: [], pending: 0, failed: 0 });
  const [loaded, setLoaded] = useState(false), [error, setError] = useState(''), [connectionError, setConnectionError] = useState('');
  const [busy, setBusy] = useState(false), [refreshing, setRefreshing] = useState(false), [pinned, setPinned] = useState(true);
  const [maximized, setMaximized] = useState(false), [focused, setFocused] = useState(true);
  const [preferences, setPreferences] = useState<Preferences>(defaults), [settingsOpen, setSettingsOpen] = useState(false), [startupError, setStartupError] = useState('');
  const [draft, setDraft] = useState<Draft | null>(null), [context, setContext] = useState<Context | null>(null);
  const locked = useRef(false), polling = useRef(false), generation = useRef(0);
  const menu = useRef<HTMLDivElement>(null), previousFocus = useRef<HTMLElement | null>(null);

  const refresh = useCallback(async () => {
    if (polling.current || locked.current) return;
    polling.current = true; const version = generation.current; setRefreshing(true);
    try {
      const next = await invoke<Snapshot>('rpc', { method: 'tasks', params: {} });
      if (version === generation.current) { setSnapshot(next); setLoaded(true); setConnectionError(''); }
    } catch (e) { if (version === generation.current) setConnectionError(message(e)); }
    finally { polling.current = false; setRefreshing(false); }
  }, []);
  useEffect(() => { void refresh(); const timer = window.setInterval(() => void refresh(), 2000); return () => window.clearInterval(timer); }, [refresh]);
  useEffect(() => {
    let disposed = false;
    const win = getCurrentWindow();
    const listeners = [win.onFocusChanged(({ payload }) => setFocused(payload)), win.onResized(() => { void win.isMaximized().then(setMaximized); setContext(null); })];
    void win.isFocused().then(v => { if (!disposed) setFocused(v); });
    void win.isMaximized().then(v => { if (!disposed) setMaximized(v); });
    void win.isAlwaysOnTop().then(v => { if (!disposed) setPinned(v); });
    void invoke<{ settings: Preferences; shortcutError?: string; startupError?: string }>('get_settings').then(result => { if (!disposed) { setPreferences(result.settings); setStartupError([result.shortcutError, result.startupError].filter(Boolean).join('；')); } }).catch(e => { if (!disposed) setError(message(e)); });
    return () => { disposed = true; for (const listener of listeners) void listener.then(unlisten => unlisten()); };
  }, []);
  useEffect(() => { if (!focused) setContext(null); }, [focused]);
  useEffect(() => { if (context) menu.current?.querySelector<HTMLButtonElement>('button')?.focus(); }, [context]);
  useLayoutEffect(() => {
    if (!context || !menu.current) return;
    const bounds = menu.current.getBoundingClientRect();
    const x = Math.max(8, Math.min(context.x, window.innerWidth - bounds.width - 8));
    const y = Math.max(8, Math.min(context.y, window.innerHeight - bounds.height - 8));
    if (x !== context.x || y !== context.y) setContext({ ...context, x, y });
  }, [context]);

  async function mutate(method: 'save' | 'complete' | 'delete', params: object, closeEditor = false) {
    if (locked.current) return;
    locked.current = true; generation.current += 1; setBusy(true); setError('');
    try {
      await invoke('rpc', { method, params });
      if (closeEditor) setDraft(null);
      try { const next = await invoke<Snapshot>('rpc', { method: 'tasks', params: {} }); setSnapshot(next); setLoaded(true); }
      catch (e) { setConnectionError(`操作已提交，刷新暂时失败：${message(e)}`); }
    } catch (e) { setError(message(e)); }
    finally { locked.current = false; setBusy(false); }
  }
  async function nativeAction(action: 'toggle_pin' | 'hide_window' | 'toggle_maximize') {
    try {
      if (action === 'toggle_pin') setPinned(await invoke<boolean>(action));
      else if (action === 'toggle_maximize') setMaximized(await invoke<boolean>(action));
      else await invoke(action);
    } catch (e) { setError(message(e)); }
  }
  function begin(next: Draft) {
    if (locked.current) return;
    if (draft && draft.text !== (draft.task?.text || '')) { setError('请先保存或取消当前任务。'); return; }
    if (next.task && !next.task.editable) { setError('此任务包含富文本，请在好用便签编辑；右键可切换象限。'); return; }
    setError(''); setDraft(next);
  }
  function save() {
    if (!draft?.text.trim() || busy) return;
    void mutate('save', { ...(draft.task ? { id: draft.task.id, update_at: draft.task.update_at, revision: draft.task.revision } : {}), text: draft.text, priority: draft.priority, account: draft.account }, true);
  }
  function openContext(e: React.MouseEvent, task?: Task, editing = false) {
    e.preventDefault(); if (busy) return;
    previousFocus.current = document.activeElement as HTMLElement;
    setContext({ x: e.clientX, y: e.clientY, task, editing, account: editing ? draft?.account : snapshot.account });
  }
  function closeContext() { setContext(null); previousFocus.current?.focus(); }
  function move(priority: Priority) {
    if (!context) return;
    if ((!context.task || context.editing) && draft) setDraft({ ...draft, priority });
    else if (context.task) void mutate('save', { id: context.task.id, update_at: context.task.update_at, revision: context.task.revision, text: context.task.text, priority, account: context.account });
    closeContext();
  }
  function deleteTask() {
    if (!context?.task || busy) return;
    const { task, account } = context;
    closeContext();
    void mutate('delete', { id: task.id, update_at: task.update_at, account }, draft?.task?.id === task.id);
  }
  const stale = draft?.task && snapshot.tasks.find(t => t.id === draft.task!.id && (t.revision && draft.task!.revision ? t.revision !== draft.task!.revision : t.update_at !== draft.task!.update_at));
  const visibleError = error || connectionError || startupError;

  return <main className={`floating-window ${focused ? 'window-focused' : 'window-unfocused'} ${maximized ? 'maximized' : ''}`}><div className="interface">
    <header className="window-header">
      <button className="icon-button" title="设置" aria-label="设置" aria-expanded={settingsOpen} onClick={() => setSettingsOpen(v => !v)}><Icon name="menu"/></button>
      <div className="drag-area" onDoubleClick={() => void nativeAction('toggle_maximize')} onMouseDown={e => { if (e.button === 0 && e.detail !== 2) void getCurrentWindow().startDragging().catch(e => setError(message(e))); }}><span className="brand-mark" aria-hidden="true"><i/><i/><i/><i/></span><h1>四象限</h1><span className="brand-subtitle">LinkinNotes</span></div>
      <span className="total-count">{snapshot.tasks.length} 项待办</span>
      <button className={`icon-button ${pinned ? 'active' : ''}`} title={pinned ? '取消置顶' : '窗口置顶'} aria-label={pinned ? '取消置顶' : '窗口置顶'} aria-pressed={pinned} onClick={() => void nativeAction('toggle_pin')}><Icon name="pin"/></button>
      <button className="icon-button" title="刷新" aria-label="刷新" disabled={refreshing || busy} onClick={() => void refresh()}><Icon name="refresh"/></button>
      <button className="icon-button" title={maximized ? '还原窗口' : '最大化窗口'} aria-label={maximized ? '还原窗口' : '最大化窗口'} onClick={() => void nativeAction('toggle_maximize')}><Icon name={maximized ? 'restore' : 'maximize'}/></button>
      <button className="icon-button" title="隐藏窗口" aria-label="隐藏窗口" onClick={() => void nativeAction('hide_window')}><Icon name="hide"/></button>
    </header>
    <section className="quadrants" aria-label="所有未完成任务">{priorities.map((priority, index) => {
      const tasks = snapshot.tasks.filter(t => t.priority === priority), inline = draft?.priority === priority ? draft : null;
      const editor = inline && <InlineEditor key={inline.task?.id || 'new'} draft={inline} busy={busy} onChange={text => setDraft({ ...inline, text })} onSave={save} onCancel={() => setDraft(null)} onContext={e => openContext(e, inline.task, true)}/>;
      return <section className={`quadrant ${priority}`} key={priority} aria-labelledby={`title-${priority}`}>
        <header className="quadrant-header"><span className="quadrant-number">0{index + 1}</span><h2 id={`title-${priority}`} title="在设置中自定义名称">{preferences.names[priority]}</h2><span className="count">{tasks.length}</span><button className="icon-button add-button" aria-label={`添加${preferences.names[priority]}任务`} title="添加任务" disabled={!loaded || busy} onClick={() => begin({ text: '', priority, account: snapshot.account })}><Icon name="plus"/></button></header>
        <div className="task-list">
          {inline && (!inline.task || inline.task.priority !== priority) && editor}
          {tasks.map(task => draft?.task?.id === task.id && draft.priority !== priority ? null : inline?.task?.id === task.id ? editor : <article className="task" key={task.id} onContextMenu={e => openContext(e, task)}>
            <button className="complete-button" aria-label={`完成：${task.text}`} title="标记完成" disabled={busy} onClick={() => void mutate('complete', { id: task.id, update_at: task.update_at, account: snapshot.account })}><span>✓</span></button>
            <button className="task-content" onClick={() => begin({ task, text: task.text, priority: task.priority, account: snapshot.account })} title="点击编辑 · 右键切换象限或删除">{task.text}{task.recurring && <small title="重复任务请在客户端完成"> ↻</small>}</button>
          </article>)}
          {!tasks.length && !inline && <p className="empty-state">{loaded ? '暂无待办' : '正在连接…'}</p>}
        </div>
      </section>;
    })}</section>
    {stale && <div className="conflict">客户端已修改此任务，请核对内容。<button onClick={() => { if (draft) setDraft({ ...draft, task: stale }); setError('已采用最新版本，请核对后保存。'); }}>保留输入，采用最新版本</button></div>}
    <footer><span className={`status ${visibleError || snapshot.failed ? 'has-error' : ''}`}><i/>{busy ? '正在处理…' : !loaded ? '正在连接' : snapshot.failed ? `${snapshot.failed} 项同步失败` : snapshot.pending ? `${snapshot.pending} 项等待同步` : '已与好用便签同步'}</span><span>{draft ? 'Enter 保存 · Shift+Enter 换行 · Esc 取消' : '点击编辑 · 右键切换象限或删除'}</span></footer>
    {visibleError && <div className="error-banner" role="alert"><span>{visibleError}</span><button onClick={() => { setError(''); setStartupError(''); void refresh(); }}>重试</button><button aria-label="关闭错误提示" onClick={() => { setError(''); setConnectionError(''); setStartupError(''); }}>×</button></div>}
    {settingsOpen && <SettingsPanel preferences={preferences} onClose={() => setSettingsOpen(false)} onSaved={next => { setPreferences(next); setStartupError(''); setSettingsOpen(false); }}/>}
    {context && <div className="context-dismiss" onMouseDown={closeContext} onContextMenu={e => { e.preventDefault(); closeContext(); }}><div className="context-menu" ref={menu} role="menu" aria-label="任务操作" style={{ left: context.x, top: context.y }} onMouseDown={e => e.stopPropagation()} onKeyDown={e => {
      if (e.key === 'Escape') { e.preventDefault(); closeContext(); }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); const items = Array.from(menu.current!.querySelectorAll<HTMLButtonElement>('button')); const index = items.indexOf(document.activeElement as HTMLButtonElement); items[(index + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]?.focus(); }
    }}><p>移动到象限</p>{priorities.map(p => <button role="menuitem" className={p} key={p} title={preferences.names[p]} onClick={() => move(p)}><i/><span className="menu-label">{preferences.names[p]}</span><span className="menu-check">{p === (context.editing ? draft?.priority : context.task?.priority || draft?.priority) ? '✓' : ''}</span></button>)}{context.task && <><div className="menu-separator" role="separator"/><button role="menuitem" className="danger-action" onClick={deleteTask}><Icon name="trash"/>删除待办</button></>}</div></div>}
  </div></main>;
}
