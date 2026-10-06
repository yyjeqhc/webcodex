#!/bin/sh
# Read-only fixture: exact npm argv, stdin EOF, and bounded deterministic output.
[ "$#" -eq 3 ] && [ "$1" = config ] && [ "$2" = get ] || exit 9
if IFS= read -r unexpected; then
    exit 10
fi
case "$3" in
    proxy) printf '%s\n' 'http://query-proxy.test:8080' ;;
    ca) printf '%s\n' null ;;
    failed) printf '%s\n' 'http://must-not-be-used.test:8080'; exit 8 ;;
    *) exit 8 ;;
esac
