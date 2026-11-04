import json
import pathlib
import sqlite3
import tempfile
import unittest

from native import NativeDatabase
from store import Store


class StoreTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.path=pathlib.Path(self.temp.name)/'test.db'
        sqlite3.connect(self.path).close()
        self.db=NativeDatabase(self.path)
        schema=pathlib.Path(__file__).parent/'fixtures/schema.sql'
        for statement in schema.read_text(encoding='utf-8').strip().split('\n\n'):
            self.db.query(statement)
        self.store=Store(self.db,{'user_id':'test-user','username':'test-user','nickname':'Test User'})
        for priority in ('ui','nui','uni','nuni'):
            task_id=self.store.mutate('save',{'text':'Synthetic task '+priority,'priority':priority})['id']
            self.db.query('UPDATE user_todo_items SET create_at=1,update_at=1 WHERE local_item_id=?',(task_id,))
        completed=self.store.mutate('save',{'text':'Synthetic completed task','priority':'ui'})['id']
        self.db.query("UPDATE user_todo_items SET done='on' WHERE local_item_id=?",(completed,))
        self.db.query('DELETE FROM operations_v1')

    def tearDown(self):
        self.db.close(); self.temp.cleanup()

    def test_chinese_create_edit_complete_and_conflict(self):
        baseline=self.db.query('SELECT * FROM user_todo_items ORDER BY local_item_id')
        title='\u4e2d\u6587\u53cc\u5411\u540c\u6b65'
        task_id=self.store.mutate('save',{'text':title,'priority':'ui'})['id']
        task=next(t for t in self.store.tasks()['tasks'] if t['id']==task_id)
        self.assertEqual(task['text'],title)
        self.assertEqual(self.db.query('SELECT local_item_id FROM search_user_todo_items_fts WHERE search_user_todo_items_fts MATCH ?',('中文',))[0]['local_item_id'],task_id)
        self.store.mutate('save',dict(task,text=title+' edited',priority='nui'))
        with self.assertRaisesRegex(ValueError,'任务已'):
            self.store.mutate('save',dict(task,text='stale'))
        current=next(t for t in self.store.tasks()['tasks'] if t['id']==task_id)
        self.store.mutate('complete',current)
        self.assertNotIn(task_id,[t['id'] for t in self.store.tasks()['tasks']])
        ops=self.db.query('SELECT operation,operation_details FROM operations_v1 WHERE operation_details LIKE ?',('%'+task_id+'%',))
        self.assertEqual(len(ops),4)
        self.assertEqual(json.loads(ops[0]['operation_details'])['created_items'][0]['content'],json.dumps({'version':'101','content':[{'type':0,'text':title}]},ensure_ascii=False,separators=(',',':')))
        self.assertEqual(baseline,self.db.query('SELECT * FROM user_todo_items WHERE local_item_id != ? ORDER BY local_item_id',(task_id,)))
        self.assertEqual(self.db.query('PRAGMA integrity_check')[0]['integrity_check'],'ok')

    def test_queue_failure_rolls_back(self):
        before=self.db.query('SELECT count(*) AS n FROM user_todo_items')[0]['n']
        self.db.query("CREATE TRIGGER fail_queue BEFORE INSERT ON operations_v1 BEGIN SELECT RAISE(ABORT,'test failure'); END")
        with self.assertRaises(RuntimeError): self.store.mutate('save',{'text':'rollback','priority':'ui'})
        self.assertEqual(before,self.db.query('SELECT count(*) AS n FROM user_todo_items')[0]['n'])

    def test_old_tasks_not_filtered_by_month(self):
        expected=self.db.query("SELECT count(*) AS n FROM user_todo_items WHERE delete_at=0 AND done='off' AND closed='off'")[0]['n']
        self.assertEqual(len(self.store.tasks()['tasks']),expected)


if __name__=='__main__': unittest.main()
