\ Mandrake loader logo, drawn to the right of the boot menu.
\
\ Selected by loader_logo="mandrake" (see conf.d/mandrake). Loaded by
\ /boot/forth/beastie.4th, which expects this file to define `logo'.
\ Text only: renders the same on the framebuffer and on a serial console.
\
\ Copyright 2026 Nightshade Systems. MIT licence, see LICENSE.

52 logoX !
3 logoY !

: logo+ ( x y c-addr/u -- x y' )
	2swap 2dup at-xy 2swap	\ position the cursor
	[char] @ escc!		\ replace @ with Esc
	type			\ print to the screen
	1+			\ increase y for next time we're called
;

: asciimandrake ( x y -- x y' )
	s" @[32m      .   |   .      "  logo+
	s"      \  \ | /  /     "  logo+
	s"    -- .-'\|/'-. --   "  logo+
	s"       `-. | .-'      "  logo+
	s"          \|/         @[m"  logo+
	s" @[33m         .-+-.        "  logo+
	s"        /  |  \       "  logo+
	s"       |   |   |      "  logo+
	s"       |  / \  |      "  logo+
	s"        \/   \/       "  logo+
	s"        /     \       "  logo+
	s"       '       '      @[m"  logo+
;

: logo ( x y -- )
	asciimandrake
	at-bl
	2drop
;
