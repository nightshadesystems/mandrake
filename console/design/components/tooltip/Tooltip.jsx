import React from 'react';
export function Tooltip({content,position='top',size,className='',children}){
  const [show,setShow]=React.useState(false);
  const cls=['clr-tooltip',position==='bottom'?'tooltip-bottom':'',size?'tooltip-'+size:'',className].filter(Boolean).join(' ');
  return <span className={cls} onMouseEnter={()=>setShow(true)} onMouseLeave={()=>setShow(false)} onFocus={()=>setShow(true)} onBlur={()=>setShow(false)}>
    {children}
    {show&&<span className="tooltip-content" role="tooltip">{content}</span>}
  </span>;
}
