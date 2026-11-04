import json
import time
import uuid


def packed(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'))


class Store:
    def __init__(self, db, identity):
        self.db = db
        self.identity = identity

    def tasks(self):
        rows = self.db.query("SELECT local_item_id AS id,text,priority,update_at,body,constant_pattern FROM user_todo_items WHERE delete_at=0 AND done='off' AND closed='off' ORDER BY top_at DESC,sort DESC")
        for row in rows:
            try:
                row['editable'] = all(part.get('type') == 0 for part in json.loads(row.pop('body'))['content'])
            except (KeyError, ValueError, TypeError):
                row['editable'] = False
            row['recurring'] = bool(row.pop('constant_pattern'))
        states = self.db.query("SELECT operation_state,count(*) AS count FROM operations_v1 WHERE operation_state IN ('wait_sync','syncing','failed') GROUP BY operation_state")
        return {'tasks': rows, 'pending': sum(r['count'] for r in states if r['operation_state'] != 'failed'), 'failed': sum(r['count'] for r in states if r['operation_state'] == 'failed')}

    def queue(self, name, task_id, details, creating=False):
        related = {'opr_infos': [{'obj_type': 4, 'obj_id': task_id, 'opr_type': 0 if creating else 1}]}
        self.db.query('INSERT INTO operations_v1 (data_version,related_object_opr_infos,operation,operation_details,operation_state,operation_id,operater,operation_at,why_failed) VALUES (?,?,?,?,?,?,?,?,?)', ('101', packed(related), name, packed(details), 'wait_sync', str(uuid.uuid4()), self.identity['user_id'], int(time.time()), ''))

    def mutate(self, method, params):
        self.db.query('BEGIN IMMEDIATE')
        try:
            result = self._mutate(method, params)
            self.db.query('COMMIT')
            return result
        except Exception:
            self.db.query('ROLLBACK')
            raise

    def _mutate(self, method, p):
        now = time.time_ns() // 1000
        task_id = p.get('id')
        if task_id:
            rows = self.db.query("SELECT * FROM user_todo_items WHERE local_item_id=? AND delete_at=0 AND done='off' AND closed='off'", (task_id,))
            if not rows or rows[0]['update_at'] != p.get('update_at'):
                raise ValueError('任务已在好用便签中变化，请刷新后重试；当前输入已保留。')
            row = rows[0]
        elif method == 'complete':
            raise ValueError('缺少任务编号')
        if method == 'complete':
            if row['constant_pattern']:
                raise ValueError('重复任务请在好用便签中完成，以保留下一次提醒。')
            keys = ['done','closed','done_at','doer_id','doer_nickname','doer_username','constant_at','create_at','next_remind_at']
            before = {k: row[k] for k in keys}
            after = dict(has_do_info=True, done='on', closed='off', done_at=now, doer_id=self.identity['user_id'], doer_nickname=self.identity['nickname'], doer_username=self.identity['username'], has_create_at=False, create_at=0, has_constant_at=False, constant_at='', next_remind_at=0)
            self.db.query("UPDATE user_todo_items SET done='on',done_at=?,doer_id=?,doer_nickname=?,doer_username=?,next_remind_at=0,update_at=? WHERE local_item_id=?", (now,self.identity['user_id'],self.identity['nickname'],self.identity['username'],now,task_id))
            self.queue('user_todo_item_do',task_id,dict(local_item_id=task_id,update_at=now,pre_modify_info=before,modify_info=after,done_item_infos=[]))
            return {'id':task_id}
        text = str(p.get('text','')).strip()
        priority = p.get('priority')
        if not text or len(text) > 10000 or priority not in ('ui','nui','uni','nuni'):
            raise ValueError('请输入任务内容并选择象限（最多 10000 字）。')
        body = packed({'version':'101','content':[{'type':0,'text':text}]})
        if not task_id:
            task_id = str(uuid.uuid4())
            columns = self.db.query('PRAGMA table_info(user_todo_items)')
            row = {c['name']: 0 if 'INT' in c['type'].upper() else '' for c in columns}
            row.update(local_item_id=task_id,user_id=self.identity['user_id'],body=body,text=text,priority=priority,done='off',closed='off',creater_id=self.identity['user_id'],creater_username=self.identity['username'],creater_nickname=self.identity['nickname'],notify_sms='off',notify_desktop_popup='off',notify_mobile_popup='off',notify_year=2000,notify_month=1,notify_day=1,top_at=now,sort=now,create_at=now,update_at=now)
            self.db.query('INSERT INTO user_todo_items ('+','.join('"'+k+'"' for k in row)+') VALUES ('+','.join('?' for k in row)+')',tuple(row.values()))
            item = {k:v for k,v in row.items() if k not in ('remote_item_id','remote_note_id','user_id','body','text','original_content','original_content_save_at')}
            item['content'] = body
            self.queue('user_todo_items_new',task_id,{'created_items':[item]},True)
        else:
            if text != row['text']:
                content = json.loads(row['body'])
                if not all(part.get('type') == 0 for part in content['content']):
                    raise ValueError('此任务包含附件或富文本，请在好用便签中编辑内容。')
                self.db.query('UPDATE user_todo_items SET body=?,text=?,update_at=? WHERE local_item_id=?',(body,text,now,task_id))
                self.queue('user_todo_item_modify_content',task_id,dict(local_item_id=task_id,update_at=now,pre_modify_info={'content':row['body']},modify_info={'content':body}))
                now += 1
            if priority != row['priority']:
                self.db.query('UPDATE user_todo_items SET priority=?,update_at=? WHERE local_item_id=?',(priority,now,task_id))
                self.queue('user_todo_item_modify_priority',task_id,dict(local_item_id=task_id,update_at=now,pre_modify_info={'priority':row['priority']},modify_info={'priority':priority}))
        return {'id':task_id}
