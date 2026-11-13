// Offline probe of the three previously selected private PDFs. Tesseract runs
// for geometry; no leased GGUF/ONNX or network inference calls are performed.
import { spawnSync } from 'node:child_process';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const root=fileURLToPath(new URL('../../',import.meta.url));
const corpus=JSON.parse(readFileSync(path.join(root,'artifacts/private/corpus.json'),'utf8'));
const directory=path.join(root,'artifacts/private',`m2-region-probes-${Date.now()}`);
mkdirSync(directory,{recursive:true,mode:0o700});
const reports=[];
for(const [category,record] of Object.entries(corpus.selected)){
  const pages=[...new Set([1,2,Math.min(10,record.pages)])].filter(p=>p<=record.pages);
  const run=spawnSync(path.join(root,'target/debug/campuslingo-ingestion'),[
    'region-probe','--pdf',record.path,'--pages',pages.join(','),'--output',directory,
  ],{cwd:root,env:process.env,encoding:'utf8',maxBuffer:1024*1024,timeout:600000});
  if(run.error||run.status!==0){
    // stderr may contain local diagnostics: retain privately, never print it.
    writeFileSync(path.join(directory,`${category}-failure.log`),run.stderr??String(run.error),{mode:0o600});
    reports.push({category,status:'BLOCKED',exit_code:run.status});
  }else{
    const report=JSON.parse(run.stdout);
    reports.push({category,...report});
  }
}
writeFileSync(path.join(directory,'index.json'),JSON.stringify({schema_version:1,observed_at:new Date().toISOString(),
  privacy:'Host-only private derivatives. No inference or upload performed.',reports},null,2)+'\n',{mode:0o600});
console.log(JSON.stringify({reports,private_evidence_directory:directory},null,2));
if(reports.some(report=>['BLOCKED','FAILED'].includes(report.status)))process.exitCode=1;
