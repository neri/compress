;; Sample code for stk1 expansion routine
;;
;; `$ nasm -f elf stk1.asm`
;;
;; LICENSE: PUBLIC DOMAIN
;;
;; TODO: This code is written for the original header and must be modified when using this library.
;;

[section .text]
[bits 32]

;;  decode s7s
;;
;; * parameters: INOUT ESI - pointer to data
;; * returns: EAX - decoded value
getnum_s7s:
    xor eax, eax
.l00:
    shl eax, 8
    lodsb
    shr eax, 1
    jnc .l00
    ret


;; check format of stk1
;;
;; `FASTCALL int tek1_checkformat(int size, void *p);`
;; * parameters: size (ECX) - size of compressed data
;; * parameters: p (EDX) - pointer to compressed data
;; * returns: EAX - positive: size of decompressed data, 
;;                  0 or negative: error
_tek1_checkformat:
    push esi
    push ecx
    push edx
    or eax, byte -1
    cmp ecx, byte 17
    jl short .err
    push edx
    pop esi
    mov ecx, 15
    lea edx, [ebp + sign_tek]
.l000:
        mov al, [esi + ecx]
        cmp al, [edx + ecx]
        jz short .l001
            mov al, -1
            or ecx, ecx
            jnz short .err
            dec eax
            jmp short .err
.l001:
        dec ecx
    jge .l000
    add esi, byte 16
    call getnum_s7s
.err:
    pop edx
    pop ecx
    pop esi
    ret


;; `FASTCALL int tek1_decode(void *p, void *q);`
;; * parameters: p (ECX) - pointer to compressed data
;; * parameters: q (EDX) - pointer to decompressed data
;; * returns: undefined
_tek1_decode:
    push ebp
    push ebx
    push esi
    push edi

    lea esi, [ecx + 16]
    mov edi, edx
    call getnum_s7s
    xchg eax, ebp
    call getnum_s7s
    ; /* EAX: bit0:must1, bit1-4:bsiz, bit5: bit6: bit7-10:MD bit11-14:MDS */
    cmp eax, 0x8000
    jae .err1
    test al, 0x01
    jz short .err1
    test al, 0x20
    jnz short .err1
    mov cl, al
    shr cl, 1
    xor edx, edx
    and ecx, byte 0x0F
    inc edx
    add cl, 8
    shl edx, cl
    cmp edx, ebp
    jb short .err1
    test al, 0x40
    jz short .l000
        call getnum_s7s
.l000:
    call getnum_s7s
    jz short .l001
.err1:
        pop edi
        pop esi
        pop ebx
        pop ebp
        xor eax, eax
        inc eax
        ret
.l001:
    or ebp, ebp
    jz .fin

    add ebp, edi
.l010:
        movzx ecx, byte [esi]
        inc esi
        mov ebx, ecx
        and ecx, byte 0x0F
        jnz .getlong_by0
            call getnum_s7s
            xchg eax, ecx
.getlong_by0:
        shr ebx, 4
        jnz .getlong_lz0
            call getnum_s7s
            xchg eax, ebx
.getlong_lz0:
        rep movsb
        cmp edi, ebp
        jae .l019
.l020:
            movzx edx, byte [esi]
            inc esi
            mov ecx, edx
            and edx, byte 0x0F
            shr edx, 1
            jc short .l022
.l021:
                    shl edx, 8
                    mov dl, [esi]
                    inc esi
                    shr edx, 1
                jnc .l021
.l022:
            shr ecx, 4
            jnz short .long_cp0
                call getnum_s7s
                xchg eax, ecx
.long_cp0:
            not edx
            inc ecx
.l023:
                mov al, [edi + edx]
                stosb
                dec ecx
            jnz short .l023
            dec ebx
        jnz short .l020
    cmp edi, ebp
    jb short .l010
.l019:

.fin:
    xor eax, eax
    pop edi
    pop esi
    pop ebx
    pop ebp
    ret


[section .rdata]
sign_tek:
    db 0x83, 0xff, 0xff, 0xff, 0x01, 0x00, 0x00, 0x00
    db 0x4f, 0x53, 0x41, 0x53, 0x4b, 0x43, 0x4d, 0x50
