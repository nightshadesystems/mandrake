import React from 'react';
export function Select({options,children,className='',...rest}){
  return <div className="clr-select-wrapper"><select className={'clr-select '+className} {...rest}>
    {options?options.map(o=>typeof o==='string'?<option key={o} value={o}>{o}</option>:<option key={o.value} value={o.value}>{o.label}</option>):children}
  </select></div>;
}
export function Datalist({options=[],className='',id,...rest}){
  const listId=id||'dl-'+React.useId();
  return <React.Fragment>
    <input className={'clr-input '+className} list={listId} {...rest}/>
    <datalist id={listId}>{options.map(o=><option key={o} value={o}></option>)}</datalist>
  </React.Fragment>;
}
