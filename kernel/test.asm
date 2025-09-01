; hello.asm -- A minimal "Hello, world!" program in 64-bit Linux assembly with NASM

section .data
    msg db "Hello, world!", 0xa  ; Our string, followed by a newline
    len equ $ - msg              ; Calculate the length of the message

section .text
    global _start

_start:
    ; Write the message to stdout.
    ; Linux syscall write = 1
    ; Arguments:
    ;   rax = syscall number
    ;   rdi = file descriptor (1 = stdout)
    ;   rsi = address of string
    ;   rdx = length of string
    mov rax, 1            ; syscall: sys_write
    mov rdi, 1            ; file descriptor: stdout
    mov rsi, msg          ; address of our message
    mov rdx, len          ; length of our message
    syscall

    ; Exit the program.
    ; Linux syscall exit = 60
    ; Arguments:
    ;   rax = syscall number
    ;   rdi = exit code (0 = success)
    mov rax, 60           ; syscall: sys_exit
    xor rdi, rdi          ; exit code 0
    syscall