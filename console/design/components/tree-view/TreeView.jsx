import React from 'react';
function Node({node,depth,activeId,onSelect}){
  const [open,setOpen]=React.useState(!!node.expanded);
  const kids=node.children||[];
  return <div className="clr-tree-node">
    <div className={'clr-tree-node-content'+(activeId===node.id?' active':'')} onClick={()=>onSelect&&onSelect(node)}>
      {kids.length>0?<button className="clr-tree-node-caret" onClick={e=>{e.stopPropagation();setOpen(o=>!o);}} aria-label="Toggle"><clr-icon shape="angle" dir={open?'down':'right'} size="10"></clr-icon></button>:<span className="clr-tree-node-caret"></span>}
      {node.icon&&<clr-icon shape={node.icon} size="14"></clr-icon>}
      <span>{node.label}</span>
    </div>
    {open&&kids.length>0&&<div className="clr-tree-children">{kids.map(k=><Node key={k.id||k.label} node={k} depth={depth+1} activeId={activeId} onSelect={onSelect}/>)}</div>}
  </div>;
}
export function TreeView({nodes,defaultActiveId,onSelect,className=''}){
  const [active,setActive]=React.useState(defaultActiveId);
  return <div className={className}>{nodes.map(n=><Node key={n.id||n.label} node={n} depth={0} activeId={active} onSelect={node=>{setActive(node.id);onSelect&&onSelect(node);}}/>)}</div>;
}
