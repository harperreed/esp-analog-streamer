
/home/harper/workspace/personal/esp-analog-streamer/target/kata-hardening/oct13-relink-parent/recipient/firmware/target/xtensa-esp32-espidf/release/esp-shazam:     file format elf32-xtensa-le


Disassembly of section .iram0.vectors:

Disassembly of section .iram0.text:

400848f0 <shazam_mp3_open>:
400848f0:	008136        	entry	a1, 64
400848f3:	019d      	mov.n	a9, a1
400848f5:	0a0c      	movi.n	a10, 0
400848f7:	780c      	movi.n	a8, 7
400848f9:	038876        	loop	a8, 40084900 <shazam_mp3_open+0x10>
400848fc:	09a9      	s32i.n	a10, a9, 0
400848fe:	994b      	addi.n	a9, a9, 4
40084900:	efa781        	l32r	a8, 4008079c <shine_get_bits_count+0x2f4> (4008471c <shine_set_config_mpeg_defaults>)
40084903:	a18b      	addi.n	a10, a1, 8
40084905:	0008e0        	callx8	a8
40084908:	280c      	movi.n	a8, 2
4008490a:	0189      	s32i.n	a8, a1, 0
4008490c:	efa581        	l32r	a8, 400807a0 <shine_get_bits_count+0x2f8> (bb80 <UserFrameTotalSize+0xba80>)
4008490f:	1189      	s32i.n	a8, a1, 4
40084911:	080c      	movi.n	a8, 0
40084913:	2189      	s32i.n	a8, a1, 8
40084915:	40a182        	movi	a8, 0x140
40084918:	3189      	s32i.n	a8, a1, 12
4008491a:	728c      	beqz.n	a2, 40084925 <shazam_mp3_open+0x35>
4008491c:	12b8      	l32i.n	a11, a2, 4
4008491e:	22c8      	l32i.n	a12, a2, 8
40084920:	0228      	l32i.n	a2, a2, 0
40084922:	000146        	j	4008492b <shazam_mp3_open+0x3b>
40084925:	ef9bc1        	l32r	a12, 40080794 <shine_get_bits_count+0x2ec> (400848d0 <host_deallocate>)
40084928:	ef9cb1        	l32r	a11, 40080798 <shine_get_bits_count+0x2f0> (400848e0 <host_allocate>)
4008492b:	ef9e81        	l32r	a8, 400807a4 <shine_get_bits_count+0x2fc> (40084480 <shine_initialise>)
4008492e:	02dd      	mov.n	a13, a2
40084930:	01ad      	mov.n	a10, a1
40084932:	0008e0        	callx8	a8
40084935:	0a2d      	mov.n	a2, a10
40084937:	f01d      	retw.n

Disassembly of section .flash.text:
