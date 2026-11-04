#!/usr/bin/env python3
"""Debug-helper resource stress with bounded stdout backpressure, not UI baseline."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from process_metrics import TreeSampler
from memcheck import Metrics, stop_tree

root=Path(__file__).resolve().parent.parent
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='text-preview-memory-',dir=base) as tmp:
    path=Path(tmp)/'large.txt'
    with path.open('wb') as output:
        output.write(b'a'*(256*1024+1));output.truncate(4*1024*1024*1024)
    process=subprocess.Popen([str(root/'target/debug/files-helper'),'preview_text'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
    sampler=TreeSampler(process.pid); metrics=Metrics(); start=time.monotonic()
    try:
        process.stdin.write(json.dumps({'command':'preview_text','id':8,'path':str(path)}).encode()+b'\n')
        process.stdin.flush()
        # Deliberately delay stdout consumption so the short-lived helper's
        # allocated response is observable. This is a backpressure stress case.
        for _ in range(20):
            metrics.add(sampler.sample());time.sleep(.025)
        output,error=process.communicate(timeout=3)
        assert process.returncode==0,error
        data=json.loads(output)
        assert data['id']==8 and data['truncated'] and data['text']=='a'*(256*1024)
        report=metrics.report(start)
        report['scenario']='debug text helper, sparse4GiB input,256KiB bounded preview,stdout backpressure500ms'
        report['scope']='helper only; not integrated UI budget, not latency; sampled peak lower bound'
        report['peak_mb_decimal']=report['sampled_maxima']['memory_kib']*1024/1e6
        report['functional_result']='PASS'
    finally:
        cleanup=stop_tree(process,sampler)
    report['cleanup']=cleanup
    evidence=root/'docs/evidence/text-preview';evidence.mkdir(parents=True,exist_ok=True)
    (evidence/'helper-memory.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'peak_mb':report['peak_mb_decimal'],'complete':report['complete'],'cleanup':cleanup['complete'],'scope':report['scope']}))
