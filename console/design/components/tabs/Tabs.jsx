import React from 'react';
export function Tabs({tabs=[],vertical,defaultIndex=0,onChange,className=''}){
  const [i,setI]=React.useState(defaultIndex);
  return <div className={['clr-tabs',vertical?'clr-tabs-vertical':'',className].filter(Boolean).join(' ')}>
    <div className="clr-tabs-list" role="tablist">
      {tabs.map((t,ti)=><button key={ti} role="tab" aria-selected={ti===i} className="clr-tab-link" disabled={t.disabled} onClick={()=>{setI(ti);onChange&&onChange(ti);}}>
        {t.icon&&<clr-icon shape={t.icon} size="14"></clr-icon>}{t.label}{t.badge!=null&&<span className="badge">{t.badge}</span>}
      </button>)}
    </div>
    <div className="clr-tab-content" role="tabpanel">{tabs[i]&&tabs[i].content}</div>
  </div>;
}
