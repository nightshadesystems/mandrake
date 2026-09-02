import React from 'react';
export function Icon({shape,size=16,solid,dir,badge,className='',style,...rest}){
  const cls=[solid?'is-solid':'',badge?'has-badge':'',className].filter(Boolean).join(' ');
  return <clr-icon shape={shape} size={String(size)} class={cls||undefined} dir={dir} style={style} {...rest}></clr-icon>;
}
