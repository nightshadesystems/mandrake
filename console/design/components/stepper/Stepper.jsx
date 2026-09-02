import React from 'react';
export function Stepper({steps=[],onFinish,className=''}){
  const [current,setCurrent]=React.useState(0);
  const [done,setDone]=React.useState(()=>new Set());
  const next=()=>{setDone(s=>new Set(s).add(current));if(current<steps.length-1)setCurrent(current+1);else onFinish&&onFinish();};
  return <div className={'clr-accordion clr-stepper '+className}>
    {steps.map((st,i)=>{const isOpen=i===current;const isDone=done.has(i)&&!isOpen;const err=st.error&&!isOpen;
      return <div key={i} className={'clr-accordion-panel'+(isOpen?' open':'')+(isDone?' complete':'')+(err?' error':'')}>
        <button className="clr-accordion-header" onClick={()=>{(done.has(i)||i<=current)&&setCurrent(i);}} aria-expanded={isOpen}>
          <span className="step-status">{isDone?<clr-icon shape="check" size="10"></clr-icon>:err?'!':i+1}</span>
          <span>{st.title}</span>
          {st.description&&<span className="clr-accordion-description">{st.description}</span>}
        </button>
        {isOpen&&<div className="clr-accordion-content">
          {st.content}
          <div style={{display:'flex',gap:8,marginTop:12}}>
            {i>0&&<button className="btn btn-sm" onClick={()=>setCurrent(i-1)}>Back</button>}
            <button className="btn btn-primary btn-sm" onClick={next}>{i===steps.length-1?'Finish':'Next'}</button>
          </div>
        </div>}
      </div>;})}
  </div>;
}
