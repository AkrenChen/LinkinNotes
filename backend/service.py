import contextlib
import ctypes
import io
import json
import os
import pathlib
import sys
import time
import psutil
from ctypes import wintypes

from keyfinder import find_keys
from native import NativeDatabase
from store import Store


class ProcessEntry(ctypes.Structure):
    _fields_ = [('dwSize',wintypes.DWORD),('cntUsage',wintypes.DWORD),('th32ProcessID',wintypes.DWORD),('th32DefaultHeapID',ctypes.c_size_t),('th32ModuleID',wintypes.DWORD),('cntThreads',wintypes.DWORD),('th32ParentProcessID',wintypes.DWORD),('pcPriClassBase',wintypes.LONG),('dwFlags',wintypes.DWORD),('szExeFile',wintypes.WCHAR*260)]


def client_pid():
    kernel = ctypes.WinDLL('kernel32',use_last_error=True)
    kernel.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel.Process32FirstW.argtypes = [wintypes.HANDLE,ctypes.POINTER(ProcessEntry)]
    kernel.Process32NextW.argtypes = [wintypes.HANDLE,ctypes.POINTER(ProcessEntry)]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    snapshot = kernel.CreateToolhelp32Snapshot(2,0)
    entry = ProcessEntry(); entry.dwSize = ctypes.sizeof(entry)
    try:
        ok = kernel.Process32FirstW(snapshot,ctypes.byref(entry))
        while ok:
            if entry.szExeFile.lower() == 'haoyongbianqian.exe':
                return entry.th32ProcessID
            ok = kernel.Process32NextW(snapshot,ctypes.byref(entry))
    finally:
        kernel.CloseHandle(snapshot)
    raise ValueError('请先打开并登录好用便签，然后点击刷新。')


class Service:
    def __init__(self):
        self.db = None
        self.pid = None
        self.next_attempt = 0

    def connect(self):
        pid = client_pid()
        directory = pathlib.Path(os.environ['USERPROFILE'])/'haoyongbianqian'/'data'
        opened = {pathlib.Path(f.path).resolve() for f in psutil.Process(pid).open_files() if f.path.lower().endswith('.db')}
        active = [p for p in directory.glob('*.db') if p.resolve() in opened and p.stem != '0'*32]
        if len(active) != 1:
            if self.db: self.db.close(); self.db = None
            raise ValueError('请在好用便签中登录一个账号后刷新。')
        source = active[0]
        if self.db and pid == self.pid and source == self.source:
            return
        if self.db:
            self.db.close(); self.db = None
        if time.monotonic() < self.next_attempt:
            raise ValueError('正在等待好用便签数据就绪，请稍后刷新。')
        self.next_attempt = time.monotonic()+10
        files = {source.name:source.read_bytes()[:4096]}
        with contextlib.redirect_stdout(io.StringIO()):
            keys = find_keys(pid,files)
        candidates=[]
        for name,key in keys.items():
            db=NativeDatabase(directory/name,key)
            try:
                identities=db.query('SELECT user_id,username,nickname FROM user_keys WHERE user_id=?',(pathlib.Path(name).stem,))
                if not identities:
                    identities=db.query('SELECT creater_id AS user_id,creater_username AS username,creater_nickname AS nickname FROM user_todo_items WHERE creater_id=? LIMIT 1',(pathlib.Path(name).stem,))
                if identities:
                    candidates.append((db,identities[0]))
                else:
                    db.close()
            except Exception:
                db.close()
        if len(candidates)!=1:
            for db,_ in candidates: db.close()
            raise ValueError('无法唯一确定已登录的好用便签账号，请只保留一个登录账号后重试。')
        self.db,identity=candidates[0]
        self.store=Store(self.db,identity)
        self.pid=pid
        self.source=source

    def call(self,method,params):
        self.connect()
        if method=='tasks':
            result=self.store.tasks()
            result['account']=self.store.identity['user_id']
            return result
        if method not in ('save','complete'): raise ValueError('未知操作')
        if params.get('account') != self.store.identity['user_id']:
            raise ValueError('好用便签账号已变化，请刷新后重新打开编辑框。')
        return self.store.mutate(method,params)


def main():
    sys.stdin.reconfigure(encoding='utf-8')
    sys.stdout.reconfigure(encoding='utf-8')
    service=Service()
    for line in sys.stdin:
        try:
            request=json.loads(line)
            response={'result':service.call(request['method'],request.get('params') or {})}
        except Exception as error:
            response={'error':str(error)}
        print(json.dumps(response,ensure_ascii=False),flush=True)


if __name__=='__main__': main()
