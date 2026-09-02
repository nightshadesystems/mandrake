import React from 'react';
export function Table({columns,rows,compact,noborder,className=''}){
  const cls=['table',compact?'table-compact':'',noborder?'table-noborder':'',className].filter(Boolean).join(' ');
  return <table className={cls}>
    <thead><tr>{columns.map(c=><th key={c.key} className={c.align==='right'?'right':'left'} style={{width:c.width}}>{c.label}</th>)}</tr></thead>
    <tbody>{rows.map((r,i)=><tr key={i}>{columns.map(c=><td key={c.key} className={c.align==='right'?'right':'left'}>{c.render?c.render(r):r[c.key]}</td>)}</tr>)}</tbody>
  </table>;
}
