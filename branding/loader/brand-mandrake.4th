\ Mandrake loader brand, drawn top-left of the boot menu.
\
\ Selected by loader_brand="mandrake" (see conf.d/mandrake). Loaded by
\ /boot/forth/brand.4th, which expects this file to define `brand'.
\ Text only: renders the same on the framebuffer and on a serial console.
\
\ Copyright 2026 Nightshade Systems. MIT licence, see LICENSE.

2 brandX ! 1 brandY ! \ Initialise brand placement defaults

: brand+ ( x y c-addr/u -- x y' )
	2swap 2dup at-xy 2swap	\ position the cursor
	[char] @ escc!		\ replace @ with Esc
	type			\ print to the screen
	1+			\ increase y for next time we're called
;

: asciitop ( x y -- x y' )
	s" @[1;35m __  __                _           _         "  brand+
	s" |  \/  | __ _ _ __   __| |_ __ __ _| | _____  "  brand+
	s" | |\/| |/ _` | '_ \ / _` | '__/ _` | |/ / _ \ "  brand+
	s" | |  | | (_| | | | | (_| | | | (_| |   <  __/ "  brand+
	s" |_|  |_|\__,_|_| |_|\__,_|_|  \__,_|_|\_\___|@[m "  brand+
	s" @[30;1m  Nightshade Systems  |  hypervisor OS on OmniOS@[m "  brand+
;

\ Print the media or release version, if the loader config set one.
\ kayak sets ooce_version on install media; nothing sets it otherwise.
: mandrakeversion ( x y -- x y' )
	s" ooce_version" getenv dup -1 = if
		drop			\ ooce_version not set
	else
		2swap 2dup at-xy 2swap	\ position at (x, y)
		2 fg b			\ green bold
		type			\ output
		me			\ mode end
		1+
	then
;

: brand ( x y -- )
	asciitop
	mandrakeversion
	2drop
;
