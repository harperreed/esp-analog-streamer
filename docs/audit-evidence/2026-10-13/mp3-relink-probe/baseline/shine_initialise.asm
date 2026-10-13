
/home/harper/workspace/personal/esp-analog-streamer/target/kata-hardening/oct13-relink-parent/recipient/firmware/target/xtensa-esp32-espidf/release/esp-shazam:     file format elf32-xtensa-le


Disassembly of section .iram0.vectors:

Disassembly of section .iram0.text:

40084480 <shine_initialise>:
40084480:	008136        	entry	a1, 64
40084483:	32b8      	l32i.n	a11, a2, 12
40084485:	0122a2        	l32i	a10, a2, 4
40084488:	f0a981        	l32r	a8, 4008072c <shine_get_bits_count+0x284> (40084450 <shine_check_config>)
4008448b:	207220        	or	a7, a2, a2
4008448e:	0008e0        	callx8	a8
40084491:	163a96        	bltz	a10, 400845f8 <shine_initialise+0x178>
40084494:	f0a7b1        	l32r	a11, 40080730 <shine_get_bits_count+0x288> (17690 <UserFrameTotalSize+0x17590>)
40084497:	20a550        	or	a10, a5, a5
4008449a:	0003e0        	callx8	a3
4008449d:	0a2d      	mov.n	a2, a10
4008449f:	155a16        	beqz	a10, 400845f8 <shine_initialise+0x178>
400844a2:	f0a391        	l32r	a9, 40080730 <shine_get_bits_count+0x288> (17690 <UserFrameTotalSize+0x17590>)
400844a5:	0a8d      	mov.n	a8, a10
400844a7:	0a0c      	movi.n	a10, 0
400844a9:	048976        	loop	a9, 400844b1 <shine_initialise+0x31>
400844ac:	0048a2        	s8i	a10, a8, 0
400844af:	881b      	addi.n	a8, a8, 1
400844b1:	f0a081        	l32r	a8, 40080734 <shine_get_bits_count+0x28c> (40083fcc <shine_subband_initialise>)
400844b4:	02ad      	mov.n	a10, a2
400844b6:	0249      	s32i.n	a4, a2, 0
400844b8:	1259      	s32i.n	a5, a2, 4
400844ba:	0008e0        	callx8	a8
400844bd:	f09e81        	l32r	a8, 40080738 <shine_get_bits_count+0x290> (4008344c <shine_mdct_initialise>)
400844c0:	02ad      	mov.n	a10, a2
400844c2:	0008e0        	callx8	a8
400844c5:	f09d81        	l32r	a8, 4008073c <shine_get_bits_count+0x294> (40083354 <shine_loop_initialise>)
400844c8:	20a220        	or	a10, a2, a2
400844cb:	0008e0        	callx8	a8
400844ce:	07c8      	l32i.n	a12, a7, 0
400844d0:	27b8      	l32i.n	a11, a7, 8
400844d2:	3798      	l32i.n	a9, a7, 12
400844d4:	4788      	l32i.n	a8, a7, 16
400844d6:	17a8      	l32i.n	a10, a7, 4
400844d8:	22c9      	s32i.n	a12, a2, 8
400844da:	9289      	s32i.n	a8, a2, 36
400844dc:	32a9      	s32i.n	a10, a2, 12
400844de:	72b9      	s32i.n	a11, a2, 28
400844e0:	8299      	s32i.n	a9, a2, 32
400844e2:	6798      	l32i.n	a9, a7, 24
400844e4:	57b8      	l32i.n	a11, a7, 20
400844e6:	196292        	s32i	a9, a2, 100
400844e9:	f09591        	l32r	a9, 40080740 <shine_get_bits_count+0x298> (86bc <UserFrameTotalSize+0x85bc>)
400844ec:	080c      	movi.n	a8, 0
400844ee:	1862b2        	s32i	a11, a2, 96
400844f1:	929a      	add.n	a9, a2, a9
400844f3:	0989      	s32i.n	a8, a9, 0
400844f5:	f09391        	l32r	a9, 40080744 <shine_get_bits_count+0x29c> (86b8 <UserFrameTotalSize+0x85b8>)
400844f8:	929a      	add.n	a9, a2, a9
400844fa:	0989      	s32i.n	a8, a9, 0
400844fc:	156282        	s32i	a8, a2, 84
400844ff:	166282        	s32i	a8, a2, 88
40084502:	176282        	s32i	a8, a2, 92
40084505:	880c      	movi.n	a8, 8
40084507:	190c      	movi.n	a9, 1
40084509:	c289      	s32i.n	a8, a2, 48
4008450b:	f08581        	l32r	a8, 40080720 <shine_get_bits_count+0x278> (400842ec <shine_find_samplerate_index>)
4008450e:	056292        	s32i	a9, a2, 20
40084511:	0008e0        	callx8	a8
40084514:	f08481        	l32r	a8, 40080724 <shine_get_bits_count+0x27c> (40084730 <shine_mpeg_version>)
40084517:	1462a2        	s32i	a10, a2, 80
4008451a:	0008e0        	callx8	a8
4008451d:	0abd      	mov.n	a11, a10
4008451f:	f08281        	l32r	a8, 40080728 <shine_get_bits_count+0x280> (40084354 <shine_find_bitrate_index>)
40084522:	82a8      	l32i.n	a10, a2, 32
40084524:	0462b2        	s32i	a11, a2, 16
40084527:	0008e0        	callx8	a8
4008452a:	4288      	l32i.n	a8, a2, 16
4008452c:	f08791        	l32r	a9, 40080748 <shine_get_bits_count+0x2a0> (3f427464 <granules_per_frame>)
4008452f:	eff671        	l32r	a7, 40080508 <shine_get_bits_count+0x60> (4000c944 <__floatsidf>)
40084532:	a08890        	addx4	a8, a8, a9
40084535:	0898      	l32i.n	a9, a8, 0
40084537:	0abd      	mov.n	a11, a10
40084539:	82a8      	l32i.n	a10, a2, 32
4008453b:	6299      	s32i.n	a9, a2, 24
4008453d:	2199      	s32i.n	a9, a1, 8
4008453f:	1362b2        	s32i	a11, a2, 76
40084542:	0007e0        	callx8	a7
40084545:	f085d1        	l32r	a13, 4008075c <shine_get_bits_count+0x2b4> (408f4000 <_etext+0x6cad28>)
40084548:	eff161        	l32r	a6, 4008050c <shine_get_bits_count+0x64> (4006358c <__muldf3>)
4008454b:	00a0c2        	movi	a12, 0
4008454e:	0006e0        	callx8	a6
40084551:	0acd      	mov.n	a12, a10
40084553:	c2a8      	l32i.n	a10, a2, 48
40084555:	01c9      	s32i.n	a12, a1, 0
40084557:	0161b2        	s32i	a11, a1, 4
4008455a:	0007e0        	callx8	a7
4008455d:	f00381        	l32r	a8, 4008056c <shine_get_bits_count+0xc4> (40002954 <__divdf3>)
40084560:	0bdd      	mov.n	a13, a11
40084562:	0acd      	mov.n	a12, a10
40084564:	11b8      	l32i.n	a11, a1, 4
40084566:	01a8      	l32i.n	a10, a1, 0
40084568:	4189      	s32i.n	a8, a1, 16
4008456a:	0008e0        	callx8	a8
4008456d:	0acd      	mov.n	a12, a10
4008456f:	21a8      	l32i.n	a10, a1, 8
40084571:	01c9      	s32i.n	a12, a1, 0
40084573:	0161b2        	s32i	a11, a1, 4
40084576:	0007e0        	callx8	a7
40084579:	f079d1        	l32r	a13, 40080760 <shine_get_bits_count+0x2b8> (40820000 <_etext+0x5f6d28>)
4008457c:	0c0c      	movi.n	a12, 0
4008457e:	0006e0        	callx8	a6
40084581:	0acd      	mov.n	a12, a10
40084583:	32a8      	l32i.n	a10, a2, 12
40084585:	21c9      	s32i.n	a12, a1, 8
40084587:	31b9      	s32i.n	a11, a1, 12
40084589:	0007e0        	callx8	a7
4008458c:	4188      	l32i.n	a8, a1, 16
4008458e:	0acd      	mov.n	a12, a10
40084590:	0bdd      	mov.n	a13, a11
40084592:	21a8      	l32i.n	a10, a1, 8
40084594:	31b8      	l32i.n	a11, a1, 12
40084596:	0008e0        	callx8	a8
40084599:	0bdd      	mov.n	a13, a11
4008459b:	0acd      	mov.n	a12, a10
4008459d:	11b8      	l32i.n	a11, a1, 4
4008459f:	01a8      	l32i.n	a10, a1, 0
400845a1:	0006e0        	callx8	a6
400845a4:	efdcc1        	l32r	a12, 40080514 <shine_get_bits_count+0x6c> (40002a78 <__fixdfsi>)
400845a7:	0161b2        	s32i	a11, a1, 4
400845aa:	0061a2        	s32i	a10, a1, 0
400845ad:	000ce0        	callx8	a12
400845b0:	1262a2        	s32i	a10, a2, 72
400845b3:	0007e0        	callx8	a7
400845b6:	0acd      	mov.n	a12, a10
400845b8:	0bdd      	mov.n	a13, a11
400845ba:	f00581        	l32r	a8, 400805d0 <shine_get_bits_count+0x128> (400026e4 <__subdf3>)
400845bd:	11b8      	l32i.n	a11, a1, 4
400845bf:	0021a2        	l32i	a10, a1, 0
400845c2:	0008e0        	callx8	a8
400845c5:	f06181        	l32r	a8, 4008074c <shine_get_bits_count+0x2a4> (80000000 <_rtc_slow_reserved_end+0x2fffe000>)
400845c8:	e2a9      	s32i.n	a10, a2, 56
400845ca:	308b80        	xor	a8, a11, a8
400845cd:	116282        	s32i	a8, a2, 68
400845d0:	f06081        	l32r	a8, 40080750 <shine_get_bits_count+0x2a8> (400636a8 <__eqdf2>)
400845d3:	f2b9      	s32i.n	a11, a2, 60
400845d5:	1062a2        	s32i	a10, a2, 64
400845d8:	0c0c      	movi.n	a12, 0
400845da:	00a0d2        	movi	a13, 0
400845dd:	0008e0        	callx8	a8
400845e0:	1acc      	bnez.n	a10, 400845e5 <shine_initialise+0x165>
400845e2:	0a62a2        	s32i	a10, a2, 40
400845e5:	f05bb1        	l32r	a11, 40080754 <shine_get_bits_count+0x2ac> (1000 <UserFrameTotalSize+0xf00>)
400845e8:	05ad      	mov.n	a10, a5
400845ea:	0003e0        	callx8	a3
400845ed:	0abd      	mov.n	a11, a10
400845ef:	aacc      	bnez.n	a10, 400845fd <shine_initialise+0x17d>
400845f1:	02bd      	mov.n	a11, a2
400845f3:	05ad      	mov.n	a10, a5
400845f5:	0004e0        	callx8	a4
400845f8:	020c      	movi.n	a2, 0
400845fa:	0010c6        	j	40084641 <shine_initialise+0x1c1>
400845fd:	f05681        	l32r	a8, 40080758 <shine_get_bits_count+0x2b0> (40080404 <shine_open_bit_stream>)
40084600:	f055c1        	l32r	a12, 40080754 <shine_get_bits_count+0x2ac> (1000 <UserFrameTotalSize+0xf00>)
40084603:	68c2a2        	addi	a10, a2, 104
40084606:	0008e0        	callx8	a8
40084609:	80a082        	movi	a8, 128
4008460c:	808280        	add	a8, a2, a8
4008460f:	00a0a2        	movi	a10, 0
40084612:	98a192        	movi	a9, 0x198
40084615:	048976        	loop	a9, 4008461d <shine_initialise+0x19d>
40084618:	0048a2        	s8i	a10, a8, 0
4008461b:	881b      	addi.n	a8, a8, 1
4008461d:	2288      	l32i.n	a8, a2, 8
4008461f:	6298      	l32i.n	a9, a2, 24
40084621:	880b      	addi.n	a8, a8, -1
40084623:	0e2966        	bnei	a9, 2, 40084635 <shine_initialise+0x1b5>
40084626:	20a192        	movi	a9, 0x120
40084629:	a8a0a2        	movi	a10, 168
4008462c:	839a80        	moveqz	a9, a10, a8
4008462f:	866292        	s32i	a9, a2, 0x218
40084632:	0002c6        	j	40084641 <shine_initialise+0x1c1>
40084635:	a8a092        	movi	a9, 168
40084638:	68a0a2        	movi	a10, 104
4008463b:	839a80        	moveqz	a9, a10, a8
4008463e:	866292        	s32i	a9, a2, 0x218
40084641:	f01d      	retw.n

Disassembly of section .flash.text:
