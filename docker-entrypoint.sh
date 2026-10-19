#!/bin/bash
set -e

mkdir -p /run/dbus
dbus-daemon --system --fork
avahi-daemon --no-chroot --daemonize

if [[ $# -eq 0 ]]; then
    echo "Welcome to 5GDescrambler!"
    echo "To proceed, you can either:"
    echo " 1) Run the entire evaluation by running ./evaluation/perform_evaluation.sh (requires samples to be downloaded to descrambler/samples!)"
    echo " 2) Run individual evaluation scripts from ./evaluation. More information on this can be found in ./evaluation/README.md"
    echo " 3) Experiment with 5GDescrambler by running \"descrambler --help\" or \"descrambler interactive\" for interactive mode!"

    exec /bin/bash
else
    exec "$@"
fi