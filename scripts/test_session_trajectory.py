import importlib.util
import io
import json
from pathlib import Path
import unittest
spec = importlib.util.spec_from_file_location('trajectory', Path(__file__).with_name('session-trajectory.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class ExportTests(unittest.TestCase):
    def test_pairs_results_and_excludes_private_bodies(self):
        rows = [dict(type='assistant',uuid='a',timestamp='2026-01-01',message=dict(content=[dict(type='tool_use',id='one',name='Bash',input={'command':'PRIVATE'})])),dict(type='user',uuid='b',timestamp='2026-01-02',message=dict(content=[dict(type='tool_result',tool_use_id='one',is_error=True,content='PRIVATE')]))]
        result=m.export(io.StringIO('\n'.join(json.dumps(x) for x in [*rows,rows[0]])))
        self.assertEqual(len(result['events']),1)
        self.assertEqual(result['events'][0]['state'],'error')
        self.assertNotIn('PRIVATE',json.dumps(result))
    def test_missing_result_is_unresolved(self):
        row=dict(type='assistant',timestamp='2026-01-01',message=dict(content=[dict(type='tool_use',id='one',name='Bash')]))
        self.assertEqual(m.export(io.StringIO(json.dumps(row)))['events'][0]['state'],'unresolved')

if __name__=='__main__':unittest.main()
