#!/usr/bin/env python3
"""Release CLI/wire contract: private files only, no resource/startup measurements."""
import argparse,hashlib,json,os,select,subprocess,tempfile,time,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
P=argparse.ArgumentParser();P.add_argument('--output',type=Path);P.add_argument('--helper',type=Path,default=ROOT/'target/release/files-helper');P.add_argument('--pages',action='store_true');args,rest=P.parse_known_args()
REPORT={'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','cases':[]}
class Contract(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='search-cli-',dir=BASE);self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        (self.root/'plain.txt').write_bytes(b'ab')
        with open(os.fsencode(self.root)+b'/raw-\xff.txt','wb') as f:f.write(b'raw')
    def invoke(self,command,kind=None,query='ext:txt'):
        request=dict(command=kind or command,id=41,path=str(self.root),query=query,hidden=False)
        r=subprocess.run([str(args.helper),command],input=json.dumps(request)+'\n',capture_output=True,text=True,timeout=10)
        messages=[json.loads(line) for line in r.stdout.splitlines()]
        REPORT['cases'].append({'command':command,'request':kind or command,'exit':r.returncode,'messages':messages,'stderr':r.stderr})
        return r,messages
    def paged(self,count,query='ext:txt',payload=b'x'):
        for i in range(count-2):(self.root/('file-'+str(i)+'.txt')).write_bytes(payload)
        child=subprocess.Popen([str(args.helper),'search_pages'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        def cleanup():
            if child.poll() is None:child.kill();child.wait(timeout=5)
            for stream in [child.stdin,child.stdout,child.stderr]:stream.close()
        self.addCleanup(cleanup)
        child.stdin.write((json.dumps(dict(command='search_pages',id=41,path=str(self.root),query=query,hidden=False))+'\n').encode());child.stdin.flush()
        return child
    def page(self,child):
        line=bytearray();deadline=time.monotonic()+10
        while b'\n' not in line:
            self.assertTrue(select.select([child.stdout],[],[],max(0,deadline-time.monotonic()))[0],'page deadline')
            chunk=os.read(child.stdout.fileno(),65536);self.assertTrue(chunk,'EOF before page');line.extend(chunk)
            self.assertLessEqual(len(line),2*1024*1024)
        self.assertTrue(line.endswith(b'\n'))
        return json.loads(line)
    def credit(self,child,sequence,id=41):
        child.stdin.write((json.dumps(dict(version=1,id=id,sequence=sequence))+'\n').encode());child.stdin.flush()
    @unittest.skipUnless(args.pages,'paged contract not requested')
    def test_paged_1207_results_credit_and_raw_exact_once(self):
        child=self.paged(1207);pages=[];seen=set();sizes=[]
        while True:
            page=self.page(child);self.assertEqual(page['sequence'],len(pages));self.assertEqual(page['version'],1);self.assertEqual(page['id'],41)
            for hit in page['report']['hits']:
                path=bytes(hit['relative'])if isinstance(hit['relative'],list)else os.fsencode(hit['relative'])
                self.assertNotIn(path,seen);seen.add(path)
            self.assertEqual(page['found_total'],len(seen));sizes.append(len(page['report']['hits']));pages.append(page)
            if not page['more']:break
            self.assertFalse(page['report']['complete']);self.assertFalse(select.select([child.stdout],[],[],.05)[0],'page published without credit')
            self.credit(child,page['sequence'])
        self.assertEqual(sizes,[500,500,207]);self.assertIn(b'raw-\xff.txt',seen);self.assertTrue(pages[-1]['report']['complete']);self.assertEqual(child.wait(timeout=5),0)
        REPORT['cases'].append(dict(command='search_pages',scope='actual pipe credit1207/raw/exact once; observed no next frame before credit; not timing/resource proof',pages=pages,exit=child.returncode))
    @unittest.skipUnless(args.pages,'paged contract not requested')
    def test_paged_exact_multiple_has_explicit_empty_terminal(self):
        child=self.paged(1000);sizes=[]
        for sequence in range(3):
            p=self.page(child);sizes.append(len(p['report']['hits']));self.assertEqual(p['sequence'],sequence)
            if sequence<2:self.assertTrue(p['more']);self.credit(child,sequence)
            else:self.assertFalse(p['more']);self.assertTrue(p['report']['complete']);self.assertEqual(p['found_total'],1000)
        self.assertEqual(sizes,[500,500,0]);self.assertEqual(child.wait(timeout=5),0)
        REPORT['cases'].append(dict(command='search_pages',scope='exact1000 terminal empty is end marker, not a lost last window',sizes=sizes,exit=0))
    @unittest.skipUnless(args.pages,'paged contract not requested')
    def test_paged_wrong_credit_and_eof_stop_without_next_page(self):
        for mode in ['wrong-id','wrong-sequence','EOF']:
            child=self.paged(503);self.assertTrue(self.page(child)['more'])
            if mode=='EOF':child.stdin.close()
            else:self.credit(child,1 if mode=='wrong-sequence'else 0,id=42 if mode=='wrong-id'else 41)
            self.assertNotEqual(child.wait(timeout=5),0);self.assertEqual(child.stdout.read(),b'')
            REPORT['cases'].append(dict(command='search_pages',scope=mode,exit=child.returncode,stderr=child.stderr.read(8192).decode(errors='replace')))
    def test_legacy_is_single_summary_and_preserves_raw_path(self):
        r,m=self.invoke('search');self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(len(m),1);self.assertEqual(m[0]['status'],'search');self.assertTrue(m[0]['report']['complete']);self.assertEqual(len(m[0]['report']['hits']),2)
        paths=[bytes(h['relative']) if isinstance(h['relative'],list) else os.fsencode(h['relative']) for h in m[0]['report']['hits']];self.assertIn(b'raw-\xff.txt',paths)
    def test_stream_admitted_and_final_agrees_with_counter_snapshot(self):
        r,m=self.invoke('search_stream',query='size:>10MiB');self.assertEqual(r.returncode,0,r.stderr);self.assertGreaterEqual(len(m),3)
        self.assertEqual(m[0]['status'],'search_progress');self.assertEqual(m[0]['progress']['visited'],0);self.assertEqual(m[-1]['status'],'search')
        self.assertTrue(all(x['id']==41 for x in m));p=m[-2]['progress'];summary=m[-1]['report'];self.assertEqual(p['visited'],2);self.assertEqual(p['metadata_reads'],2);self.assertEqual(p['found'],len(summary['hits']));self.assertEqual(summary['visited'],p['visited']);self.assertTrue(summary['complete'])
    def test_literal_content_binary_partial_and_raw_wire(self):
        (self.root/'yes.txt').write_text('head\nHola "mundo"\n')
        (self.root/'binary.txt').write_bytes(b'Hola "mundo"\0')
        (self.root/'large.txt').write_bytes(b'x'*(1024*1024+1))
        r,m=self.invoke('search_stream',query='ext:txt content:'+json.dumps('Hola "mundo"'))
        self.assertEqual(r.returncode,0,r.stderr);report=m[-1]['report'];self.assertFalse(report['complete']);self.assertEqual(report['content']['binary'],1);self.assertEqual(report['content']['limited'],1)
        self.assertEqual(len(report['hits']),1);self.assertEqual(report['hits'][0]['content'],dict(line=2,byte=5))
        r,m=self.invoke('search',query='name:raw content:"raw"');self.assertTrue(m[-1]['report']['complete']);self.assertEqual(len(m[-1]['report']['hits']),1)
        self.assertEqual(bytes(m[-1]['report']['hits'][0]['relative']),b'raw-\xff.txt')
    def test_regex_whole_utf8_file_cross_chunk_and_zero_width_empty_eof(self):
        (self.root/'empty.rx').write_bytes(b'')
        payload=b'x'*32767+'á\n日本\nerror42\nend'.encode().replace(b'\n',bytes([10]))
        (self.root/'good.rx').write_bytes(payload)
        (self.root/'binary.rx').write_bytes(b'error42'+bytes([0]))
        (self.root/'invalid.rx').write_bytes(b'error42'+bytes([255]))
        (self.root/'large.rx').write_bytes(b'x'*(1024*1024+1))
        r,m=self.invoke('search_batches',query='ext:rx regex:"(?s)á.*error[0-9]+"')
        self.assertEqual(r.returncode,0,r.stderr);report=m[-1]['report']
        self.assertTrue(report['content']['regex']);self.assertEqual(report['content']['binary'],2);self.assertEqual(report['content']['limited'],1);self.assertFalse(report['complete'])
        self.assertEqual(len(report['hits']),1);hit=report['hits'][0];self.assertEqual(hit['content']['byte'],32767);self.assertEqual(hit['content']['regex_end'],len(payload)-4)
        self.assertTrue(hit['snippet']['text'].encode()[hit['snippet']['start']:hit['snippet']['end']].endswith(b'error42'))
        r,m=self.invoke('search_stream',query='ext:rx regex:'+chr(92)+'z');self.assertEqual(r.returncode,0,r.stderr);hits=m[-1]['report']['hits']
        self.assertEqual(len(hits),2);self.assertTrue(all(h['content']['byte']==h['content']['regex_end'] and h.get('snippet') is None for h in hits))
        r,m=self.invoke('search',query='ext:rx regex:"^$"');self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(len(m[-1]['report']['hits']),1)
        self.assertEqual((self.root/'good.rx').read_bytes(),payload)
    def test_regex_invalid_patterns_are_explicit_errors_not_empty_results(self):
        for query in ['regex:(',r'regex:\p{L}','regex:a content:b','regex:'+('x'*1025)]:
            r,m=self.invoke('search_batches',query=query);self.assertEqual(r.returncode,0,r.stderr)
            self.assertEqual(len(m),1);self.assertEqual(m[0]['status'],'error');self.assertTrue(m[0]['message'])
    @unittest.skipUnless(args.pages,'paged contract not requested')
    def test_regex_pages_503_empty_files_keep_mode_and_zero_spans_through_credit(self):
        (self.root/'plain.txt').write_bytes(b'')
        with open(os.fsencode(self.root)+b'/raw-'+bytes([255])+b'.txt','wb') as out:out.write(b'')
        child=self.paged(503,query='ext:txt regex:'+chr(92)+'z',payload=b'');pages=[]
        for sequence in range(2):
            page=self.page(child);pages.append(page);self.assertEqual(page['sequence'],sequence)
            report=page['report'];self.assertTrue(report['content']['regex']);self.assertEqual(report['content']['bytes'],0)
            self.assertTrue(all(h['content']==dict(line=1,byte=0,regex_end=0) and h.get('snippet') is None for h in report['hits']))
            if sequence==0:self.assertTrue(page['more']);self.credit(child,0)
            else:self.assertFalse(page['more']);self.assertTrue(report['complete']);self.assertEqual(page['found_total'],503)
        self.assertEqual([len(p['report']['hits']) for p in pages],[500,3]);self.assertEqual(child.wait(timeout=5),0)
        REPORT['cases'].append(dict(command='search_pages',scope='regex503 empty files/two credited pages/zero total bytes/real spans, terminal mode retained',pages=pages,exit=0))
    def test_command_request_mismatch_and_query_error_are_explicit(self):
        r,m=self.invoke('search_stream','search');self.assertNotEqual(r.returncode,0);self.assertFalse(m)
        r,m=self.invoke('search_stream',query='size:invalid');self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(len(m),1);self.assertEqual(m[0]['status'],'error')
if __name__=='__main__':
    suite=unittest.defaultTestLoader.loadTestsFromTestCase(Contract);result=unittest.TextTestRunner(verbosity=2).run(suite);REPORT['functional_gate']='PASS' if result.wasSuccessful() else 'FAIL'
    REPORT['helper_path']=str(args.helper);REPORT['pages_requested']=args.pages
    REPORT['helper_sha256']=hashlib.sha256(args.helper.read_bytes()).hexdigest()
    if args.output:args.output.write_text(json.dumps(REPORT,ensure_ascii=False,indent=2)+'\n')
    raise SystemExit(0 if result.wasSuccessful() else 1)
