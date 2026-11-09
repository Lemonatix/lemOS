#!/bin/sh
# Bootet lemOS ohne Bildschirm in QEMU und prüft:
#   1. dass der Kernel startet und der Timer tickt ("lemOS: boot ok"),
#   2. dass die Shell Tastendrücke annimmt und "echo" ausführt,
#   3. dass der Quanten-Simulator einen Bell-Zustand erzeugt und 16 Qubits
#      in den Speicher passen,
#   4. dass "reboot" den Rechner neu startet (QEMU beendet sich dann wegen
#      -no-reboot von selbst).
#
# Aufruf: scripts/boot-test.sh <qemu> <qemu-optionen...>
# (wird von "make test" aufgerufen)

set -u
LOG=build/serial.log
MONITOR=build/monitor.fifo

rm -f "$LOG" "$MONITOR"
mkfifo "$MONITOR"

# Über den QEMU-Monitor können wir Tasten "drücken" (sendkey).
"$@" -display none -serial file:"$LOG" -monitor stdio <"$MONITOR" >/dev/null &
qemu=$!
exec 3>"$MONITOR"

finish() {
    exec 3>&-
    kill "$qemu" 2>/dev/null
    rm -f "$MONITOR"
    cat "$LOG" 2>/dev/null
    echo
    echo "$1"
    exit "$2"
}

# Wartet bis zu 30 Sekunden darauf, dass $1 in der seriellen Ausgabe steht.
wait_for() {
    for _ in $(seq 30); do
        grep -q "$1" "$LOG" 2>/dev/null && return 0
        sleep 1
    done
    return 1
}

wait_for "lemOS: boot ok" || finish "Boot-Test fehlgeschlagen: Kernel startet nicht." 1

# Tippt die Tasten $@ (Namen wie beim QEMU-Befehl sendkey).
type_keys() {
    for key in "$@"; do
        echo "sendkey $key" >&3
        sleep 0.2
    done
}

wait_for "lemsh> " || finish "Boot-Test fehlgeschlagen: Shell startet nicht." 1
type_keys e c h o spc h a l l o ret
wait_for "^hallo" || finish "Boot-Test fehlgeschlagen: Tastatur oder echo funktioniert nicht." 1

# Bell-Zustand: |00> und |11> mit je 50 %.
type_keys q spc b e l l ret
wait_for "|11>  +0.7071" || finish "Boot-Test fehlgeschlagen: q bell liefert keinen Bell-Zustand." 1

# 16 Qubits, alle in Überlagerung, dann messen.
type_keys q spc i n i t spc 1 6 ret
wait_for "16 qubits ready" || finish "Boot-Test fehlgeschlagen: 16 Qubits passen nicht in den Speicher." 1
type_keys q spc h spc 1 5 ret q spc m e a s u r e ret
wait_for "Result: |" || finish "Boot-Test fehlgeschlagen: Messung über 16 Qubits klappt nicht." 1

type_keys r e b o o t ret
for _ in $(seq 10); do
    kill -0 "$qemu" 2>/dev/null || break
    sleep 1
done
kill -0 "$qemu" 2>/dev/null && finish "Boot-Test fehlgeschlagen: reboot startet nicht neu." 1

finish "Boot-Test bestanden." 0
