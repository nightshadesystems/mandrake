import React from 'react';
export function Wizard({open,title,steps=[],onClose,onFinish,className=''}){
  const [current,setCurrent]=React.useState(0);
  const [done,setDone]=React.useState(()=>new Set());
  if(!open)return null;
  const last=current===steps.length-1;
  const next=()=>{setDone(s=>new Set(s).add(current));last?(onFinish&&onFinish()):setCurrent(current+1);};
  return <React.Fragment>
    <div className="modal-backdrop" onClick={onClose}></div>
    <div className={'modal clr-wizard '+className}>
      <div className="modal-dialog"><div className="modal-content" role="dialog" aria-modal="true">
        <div className="clr-wizard-stepnav">
          <div className="wizard-title">{title}</div>
          {steps.map((s,i)=><div key={i} className={'clr-wizard-stepnav-item'+(i===current?' active':'')+(done.has(i)?' complete':'')} onClick={()=>(done.has(i)||i<=current)&&setCurrent(i)}>
            {done.has(i)&&i!==current?<clr-icon class="step-check" shape="check-circle" size="14"></clr-icon>:<span className="step-num">{i+1}</span>}
            {s.navTitle||s.title}
          </div>)}
        </div>
        <div className="clr-wizard-content">
          <div className="modal-header"><span className="modal-title">{steps[current].title}</span><button className="close" aria-label="Close" onClick={onClose}>×</button></div>
          <div className="modal-body">{steps[current].content}</div>
          <div className="modal-footer">
            <button className="btn btn-link-neutral" onClick={onClose}>Cancel</button>
            {current>0&&<button className="btn" onClick={()=>setCurrent(current-1)}>Back</button>}
            <button className="btn btn-primary" onClick={next}>{last?'Finish':'Next'}</button>
          </div>
        </div>
      </div></div>
    </div>
  </React.Fragment>;
}
