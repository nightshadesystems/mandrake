import React from 'react';
function Block({block}){
  const [open,setOpen]=React.useState(!!block.expanded);
  const kids=block.children||[];
  return <div className={'stack-block'+(kids.length?' stack-block-expandable':'')}>
    <div className="stack-block-label" onClick={()=>kids.length&&setOpen(o=>!o)}>
      <span className="stack-block-caret">{kids.length>0&&<clr-icon shape="angle" dir={open?'down':'right'} size="10"></clr-icon>}</span>
      <span className="stack-view-key">{block.key}</span>
      <span className="stack-view-value">{block.value}</span>
    </div>
    {open&&kids.length>0&&<div className="stack-children">{kids.map((k,i)=><div key={i} className="stack-block"><div className="stack-block-label"><span className="stack-block-caret"></span><span className="stack-view-key">{k.key}</span><span className="stack-view-value">{k.value}</span></div></div>)}</div>}
  </div>;
}
export function StackView({blocks,className=''}){
  return <div className={'stack-view '+className}>{blocks.map((b,i)=><Block key={i} block={b}/>)}</div>;
}
