#!/bin/sh
# Bootet lemOS ohne Bildschirm in QEMU und prüft:
#   1. dass der Kernel startet und der Timer tickt ("lemOS: boot ok"),
#   2. dass Tastendrücke ankommen und auf dem Bildschirm erscheinen.
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

for key in h a l l o; do
    echo "sendkey $key" >&3
    sleep 0.2
done
wait_for "Tippe etwas: hallo" || finish "Boot-Test fehlgeschlagen: Tastatur reagiert nicht." 1

finish "Boot-Test bestanden." 0
