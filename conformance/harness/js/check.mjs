#!/usr/bin/env node
import fs from 'node:fs';
import { celEnv, celList, mapType, listType, CelScalar, plan, parse, celFunc, isCelError } from '@bufbuild/cel';

const I = CelScalar.INT, D = CelScalar.DOUBLE, B = CelScalar.BOOL, S = CelScalar.STRING;
const LD = listType(CelScalar.DYN);
const M = mapType(S, CelScalar.DYN);
const MAX = 9223372036854775807n, MIN = -9223372036854775808n;
const err = (message = 'evaluation error') => ({ error: message });
const isError = x => x && typeof x === 'object' && Object.prototype.hasOwnProperty.call(x, 'error');

function fromTyped(x) {
  if (!x || typeof x !== 'object') throw new Error('invalid typed value');
  const k = Object.keys(x)[0];
  if (k === 'int') return { t:'int', v:BigInt(x[k]) };
  if (k === 'double') return { t:'double', v: x[k] === 'inf' ? Infinity : x[k] === '-inf' ? -Infinity : x[k] === 'nan' ? NaN : Number(x[k]) };
  if (k === 'bool') return { t:'bool', v:Boolean(x[k]) };
  if (k === 'string') return { t:'string', v:String(x[k]) };
  if (k === 'list') return { t:'list', v:x[k].map(fromTyped) };
  if (k === 'map') return { t:'map', v:Object.fromEntries(Object.entries(x[k]).map(([a,b])=>[a,fromTyped(b)])) };
  throw new Error(`invalid typed value kind ${k}`);
}
function toCel(v) {
  if (v.t === 'int') return v.v;
  if (v.t === 'double' || v.t === 'bool' || v.t === 'string') return v.v;
  if (v.t === 'list') return v.v.map(toCel);
  if (v.t === 'map') return Object.fromEntries(Object.entries(v.v).map(([k,x])=>[k,toCel(x)]));
  throw new Error('unknown value');
}
function fromCel(v) {
  if (typeof v === 'bigint') return {t:'int',v};
  if (typeof v === 'number') return {t:'double',v};
  if (typeof v === 'boolean') return {t:'bool',v};
  if (typeof v === 'string') return {t:'string',v};
  if (v && typeof v === 'object' && typeof v[Symbol.iterator] === 'function' && 'size' in v) return {t:'list',v:[...v].map(fromCel)};
  if (v && typeof v === 'object') return {t:'map',v:Object.fromEntries(Object.entries(v).map(([k,x])=>[k,fromCel(x)]))};
  throw new Error('unknown CEL result');
}
function typedJson(v) {
  if (v.t === 'int') return {int:Number(v.v)};
  if (v.t === 'double') return {double:Number.isNaN(v.v)?'nan':v.v===Infinity?'inf':v.v===-Infinity?'-inf':v.v};
  if (v.t === 'bool') return {bool:v.v};
  if (v.t === 'string') return {string:v.v};
  if (v.t === 'list') return {list:v.v.map(typedJson)};
  if (v.t === 'map') return {map:Object.fromEntries(Object.entries(v.v).map(([k,x])=>[k,typedJson(x)]))};
}
function same(a,b) { return JSON.stringify(a) === JSON.stringify(b); }
function valueEq(a,b) { return a.t===b.t && (a.t==='double' ? (Number.isNaN(a.v)&&Number.isNaN(b.v) || Object.is(a.v,b.v)) : a.t==='list' ? a.v.length===b.v.length&&a.v.every((x,i)=>valueEq(x,b.v[i])) : a.t==='map' ? Object.keys(a.v).length===Object.keys(b.v).length&&Object.keys(a.v).every(k=>b.v[k]&&valueEq(a.v[k],b.v[k])) : a.v===b.v); }
function normalizeActivation(a) { return {t:'map',v:Object.fromEntries(Object.entries(a ?? {}).map(([k,v])=>[k,fromTyped(v)]))}; }
function pathGet(activation, path) { let x=activation; for (const p of path.split('.')) { if (!x || x.t !== 'map' || !(p in x.v)) return err('absent key'); x=x.v[p]; } return x; }
function factMatch(f, rel, args) { return f.rel===rel && f.args.length===args.length && f.args.every((x,i)=>args[i].t==='string'&&args[i].v==='_' || valueEq(fromTyped(x),args[i])); }
function host(line, name, args) {
  const str = x => x?.t==='string' ? x.v : undefined, list = x => x?.t==='list' ? x.v : undefined;
  const rel=str(args[0]), av=list(args[1]);
  if (name !== 'now' && name !== 'visited' && (rel===undefined || av===undefined)) return err('invalid host arguments');
  if (name === 'now') return line.now === undefined ? err('now unavailable') : {t:'int',v:BigInt(line.now)};
  if (name === 'visited') return {t:'bool',v:line.visited?.includes(str(args[0])) ?? false};
  const matches=(line.facts??[]).filter(f=>factMatch(f,rel,av));
  if (name === 'holds') return {t:'bool',v:matches.length>0};
  if (name === 'count') return {t:'int',v:BigInt(matches.length)};
  if (name === 'countDistinct') {
    const col=args[2]?.t==='int' ? Number(args[2].v) : -1;
    if (col<0 || !av[col] || av[col].t!=='string' || av[col].v!=='_') return err('invalid distinct column');
    const vals=[]; for (const f of matches) { const v=fromTyped(f.args[col]); if (!vals.some(x=>valueEq(x,v))) vals.push(v); }
    return {t:'int',v:BigInt(vals.length)};
  }
  if (name === 'validAt') {
    const t=args[2]?.t==='int'?args[2].v:null; if (t===null) return err('invalid time');
    return {t:'bool',v:matches.some(f=>BigInt(f.validFrom??0)<=t && (f.validTo===undefined || t<BigInt(f.validTo)))};
  }
  return err('unknown host function');
}
function exprEval(line, n) {
  try { return exprEval0({...line, activation:normalizeActivation(line.activation)},n); } catch (e) { return err(e.message); }
}
function exprEval0(line,n) {
  if ('int' in n) return {t:'int',v:BigInt(n.int)}; if ('double' in n) return {t:'double',v:Number(n.double==='inf'?Infinity:n.double==='-inf'?-Infinity:n.double==='nan'?NaN:n.double)}; if ('bool' in n) return {t:'bool',v:n.bool}; if ('string' in n) return {t:'string',v:n.string};
  if ('path' in n) return pathGet(line.activation,n.path); if ('has' in n) return {t:'bool',v:!isError(pathGet(line.activation,n.has))};
  if ('index' in n) { const a=exprEval0(line,n.index), k=exprEval0(line,n.key); if(isError(a)||isError(k)) return err('absent key'); if(a.t==='map'&&k.t==='string'&&k.v in a.v)return a.v[k.v]; if(a.t==='list'&&k.t==='int'&&k.v>=0n&&k.v<BigInt(a.v.length))return a.v[Number(k.v)]; return err('absent key'); }
  if ('list' in n) { const v=n.list.map(x=>exprEval0(line,x)); return v.some(isError)?err('list element error'):{t:'list',v}; }
  if ('call' in n) { const as=n.args.map(x=>exprEval0(line,x)); if(as.some(isError)) return err('argument error'); if(n.call==='int'&&as[0]?.t==='double'){const z=Math.trunc(as[0].v);if(!Number.isFinite(z))return err('invalid int conversion');const q=BigInt(z);return q<MIN||q>MAX?err('integer overflow'):{t:'int',v:q};} if(n.call==='int'&&as[0]?.t==='int')return as[0]; if(n.call==='double'&&(as[0]?.t==='int'||as[0]?.t==='double'))return{t:'double',v:Number(as[0].v)}; return ['holds','count','countDistinct','validAt','now','visited'].includes(n.call)?host(line,n.call,as):err('invalid call'); }
  if ('cond' in n) { const c=exprEval0(line,n.cond); if(isError(c)) return c; return c.t==='bool'?exprEval0(line,c.v?n.then:n.else):err('condition not bool'); }
  if ('op' in n && (n.op==='!'||(n.op==='-'&&!('r' in n)))) { const a=exprEval0(line,n.l); if(isError(a))return a; if(n.op==='!'&&a.t==='bool')return{t:'bool',v:!a.v}; if(n.op==='-'&&(a.t==='int'||a.t==='double')){if(a.t==='int'&&a.v===MIN)return err('integer overflow');return{t:a.t,v:-a.v};} return err('invalid unary'); }
  if ('op' in n) return binEval(line,n.op,n.l,n.r);
  return err('invalid expr node');
}
function binEval(line,op,ln,rn) {
  const l=exprEval0(line,ln); if(op==='&&'&& !isError(l)&&l.t==='bool'&&!l.v)return{t:'bool',v:false}; if(op==='||'&&!isError(l)&&l.t==='bool'&&l.v)return{t:'bool',v:true};
  const r=exprEval0(line,rn); if(op==='&&'||op==='||'){if(!isError(l)&&l.t==='bool'&&!isError(r)&&r.t==='bool')return{t:'bool',v:op==='&&'?l.v&&r.v:l.v||r.v}; if(isError(l)&&!isError(r)&&r.t==='bool'&&((op==='&&'&&!r.v)||(op==='||'&&r.v)))return r; if(isError(r)&&!isError(l)&&l.t==='bool'&&((op==='&&'&&!l.v)||(op==='||'&&l.v)))return l; return err('logical error');}
  if(isError(l)||isError(r))return err('operand error'); if(op==='in'){if(r.t==='list')return{t:'bool',v:r.v.some(x=>valueEq(l,x))};if(r.t==='map'&&l.t==='string')return{t:'bool',v:l.v in r.v};return err('invalid in');}
  if(['==','!='].includes(op)){const e=valueEq(l,r);return{t:'bool',v:op==='=='?e:!e};} if(l.t!==r.t)return err('mixed types');
  if(['<','<=','>','>='].includes(op)){if(!['int','double','string'].includes(l.t))return err('invalid compare');const x=l.v,y=r.v;return{t:'bool',v:op==='<'?x<y:op==='<='?x<=y:op==='>'?x>y:x>=y};}
  if(!['+','-','*','/','%'].includes(op)||!['int','double'].includes(l.t))return err('invalid arithmetic');
  if(l.t==='int'){if((op==='/'||op==='%')&&r.v===0n)return err('division by zero');let x=op==='+'?l.v+r.v:op==='-'?l.v-r.v:op==='*'?l.v*r.v:op==='/'?l.v/r.v:l.v%r.v;if(x<MIN||x>MAX)return err('integer overflow');return{t:'int',v:x};}
  if(op==='/'&&r.v===0)return{t:'double',v:l.v===0?NaN:l.v>0?Infinity:-Infinity}; let x=op==='+'?l.v+r.v:op==='-'?l.v-r.v:op==='*'?l.v*r.v:l.v/r.v; return{t:'double',v:x};
}
function makeCel(line) {
  const env=celEnv({variables:Object.fromEntries((line.env?.variables??[]).map(x=>[x.name,M])) , funcs:[
    celFunc('holds',[S,LD],B,(rel,args)=>host(line,'holds',[{t:'string',v:rel},fromCel(args)]).v),
    celFunc('count',[S,LD],I,(rel,args)=>host(line,'count',[{t:'string',v:rel},fromCel(args)]).v),
    celFunc('countDistinct',[S,LD,I],I,(rel,args,col)=>host(line,'countDistinct',[{t:'string',v:rel},fromCel(args),{t:'int',v:col}]).v),
    celFunc('validAt',[S,LD,I],B,(rel,args,t)=>host(line,'validAt',[{t:'string',v:rel},fromCel(args),{t:'int',v:t}]).v),
    celFunc('now',[],I,()=>host(line,'now',[]).v),
    celFunc('visited',[S],B,id=>host(line,'visited',[{t:'string',v:id}]).v),
  ]});
  return plan(env,parse(line.cel));
}
function celEval(line) { try { const p=makeCel(line); const bindings=Object.fromEntries(Object.entries(line.activation??{}).map(([k,v])=>[k,toCel(fromTyped(v))])); const out=p(bindings); if(isCelError(out))return err(out.message??String(out)); return fromCel(out); } catch(e) { return err(e.message); } }
function expected(line) { return 'error' in line.result ? err(line.result.error) : fromTyped(line.result); }
function comparable(x) { return isError(x)?'error':typedJson(x); }
async function main() {
  const files = process.argv.slice(2);
  if (!files.length) { console.error('usage: node check.mjs <dump.jsonl>...'); process.exit(2); }
  const mismatches = [];
  let lineNo = 0;
  for (const file of files) {
    const rl = (await import('node:readline')).createInterface({
      input: fs.createReadStream(file), crlfDelay: Infinity,
    });
    let env = {};
    for await (const text of rl) {
      if (!text.trim()) continue;
      let line;
      try { line = JSON.parse(text); } catch (e) {
        mismatches.push({ line: ++lineNo, cel: '<json>', expected: 'valid JSON', got: e.message });
        continue;
      }
      if (line.kind === 'env') { env = line.env ?? {}; continue; }
      line.env = env;
      lineNo++;
      const want = expected(line), gotCel = celEval(line), gotExpr = exprEval(line, line.expr);
      if (JSON.stringify(comparable(want)) !== JSON.stringify(comparable(gotCel)) ||
          JSON.stringify(comparable(want)) !== JSON.stringify(comparable(gotExpr))) {
        mismatches.push({ line: lineNo, cel: line.cel, expected: comparable(want),
          celEvaluator: comparable(gotCel), exprEvaluator: comparable(gotExpr) });
      }
    }
  }
  if (mismatches.length) { console.error(JSON.stringify({ mismatches }, null, 2)); process.exit(1); }
  console.log(`ok: ${lineNo} condition evaluations`);
}
main();
