	.file	"offset.c"
	.text
	.global	ext_offset
	.section	.rodata
	.align	4
	.type	ext_offset, @object
	.size	ext_offset, 4
ext_offset:
	.word	88
	.ident	"GCC: (crosstool-NG esp-14.2.0_20260121) 14.2.0"
