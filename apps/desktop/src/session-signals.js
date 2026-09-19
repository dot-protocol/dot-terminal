// Content-free observations for ONE view. A parsed byte is not a displayed pixel,
// and a response cursor is not the keeper's current output head.
export class SessionSignals {
  constructor(clock=()=>performance.now()) { this.clock=clock; this.reset(); }
  reset(kind='none') {
    this.kind=kind; this.received=0; this.applied=0; this.gaps=0;
    this.last=null; this.failed=false; this.samples={read:[],parse:[],input:[],resize:[]};
  }
  sample(stage,ms) {
    if(!(stage in this.samples)||!Number.isFinite(ms)||ms<0)return;
    const values=this.samples[stage]; values.push(Math.round(ms));
    if(values.length>120)values.shift();
  }
  receive(next,gap=false) {
    if(!Number.isSafeInteger(next)||next<0)throw new Error('Invalid output cursor');
    if(next<this.received)throw new Error('Output cursor moved backwards');
    this.received=next; this.last=this.clock();this.failed=false;
    if(gap)this.gaps++;
  }
  apply(next) { this.applied=Math.max(this.applied,Math.min(next,this.received)); }
  fail() { this.failed=true; }
  snapshot() {
    const age=this.last===null?null:Math.max(0,Math.round(this.clock()-this.last));
    return {schema:'dot.session-view.v1',coverage:'local-view',kind:this.kind,
      state:this.failed?'error':age===null?'unknown':age>3000?'stale':this.gaps?'history-gap':this.received>this.applied?'catching-up':'caught-up-to-response',
      responseAgeMs:age,receivedOffset:this.received,appliedOffset:this.applied,
      pendingParseBytes:this.received-this.applied,historyGaps:this.gaps,
      peerViews:'unknown',hostHead:'unknown',pixelLatencyMs:null,
      latency:Object.fromEntries(Object.entries(this.samples).map(([stage,values])=>{
        const a=[...values].sort((x,y)=>x-y);
        return [stage,{count:a.length,p50:a.length?a[Math.ceil(a.length*.5)-1]:null,p95:a.length?a[Math.ceil(a.length*.95)-1]:null}];
      }))};
  }
}

// Coalesce layout storms; never allow an older resize to finish after a newer one.
export class LatestResize {
  constructor(send,onError=()=>{}) {this.send=send;this.onError=onError;this.pending=null;this.running=false;this.last=null;}
  request(value) {this.pending=value;return this.drain();}
  async drain() {
    if(this.running)return;
    this.running=true;
    try {while(this.pending){const value=this.pending;this.pending=null;
      const key=JSON.stringify(value);if(key===this.last)continue;
      try{await this.send(value);this.last=key;}catch(e){this.onError(e,value);}
    }}finally{this.running=false;}
  }
}
