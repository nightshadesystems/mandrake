import React from 'react';
const nodeIcon={success:<clr-icon shape="check" size="12"></clr-icon>,error:<clr-icon shape="times" size="12"></clr-icon>,current:<clr-icon shape="circle" size="10" class="is-solid"></clr-icon>,processing:<span className="spinner spinner-sm" style={{width:24,height:24,borderWidth:3}}></span>};
export function Timeline({steps=[],vertical,className=''}){
  return <div className={['clr-timeline',vertical?'clr-timeline-vertical':'',className].filter(Boolean).join(' ')}>
    {steps.map((s,i)=><div key={i} className={'clr-timeline-step'+(s.state?' '+s.state:'')}>
      <div className="clr-timeline-step-node">{s.state?nodeIcon[s.state]:null}</div>
      <div className="clr-timeline-step-body">
        {s.header&&<div className="clr-timeline-step-header">{s.header}</div>}
        <div className="clr-timeline-step-title">{s.title}</div>
        {s.description&&<div className="clr-timeline-step-description">{s.description}</div>}
      </div>
    </div>)}
  </div>;
}
