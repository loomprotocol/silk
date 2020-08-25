#!/bin/bash

# Start the 1st process - faucet
NDEBUG=1 ./multinode-demo/faucet.sh 2>&1 | tee /tmp/solana/itt-faucet.log &
status=$?
if [ $status -ne 0 ]; then
  echo "Failed to start faucet: $status"
  exit $status
fi

sleep 15s

# Start the 2nd process - bootstrap validator
NDEBUG=1 ./multinode-demo/bootstrap-validator.sh 2>&1 | tee /tmp/solana/itt-bootstrap-validator.log &
status=$?
if [ $status -ne 0 ]; then
  echo "Failed to bootstrap validator: $status"
  exit $status
fi

sleep 15s

# Start the 3rd process - bench tps
NDEBUG=1 ./multinode-demo/bench-tps.sh 2>&1  | tee /tmp/solana/itt-bench-tps.log &
status=$?
if [ $status -ne 0 ]; then
  echo "Failed to bench tps: $status"
  exit $status
fi

sleep 15s

# exit when 1/3 processes exits, otherwise keep going
while sleep 60; do
  ps aux |grep solana-faucet |grep -q -v grep
  P1_STATUS=$?
  ps aux |grep solana-validator |grep -q -v grep
  P2_STATUS=$?
  ps aux |grep solana-bench-tps |grep -q -v grep
  P3_STATUS=$?
  # If the greps above find anything, they exit with 0 status
  # If they are not 0, then something is wrong
  if [ $P2_STATUS -ne 0 -o $P3_STATUS -ne 0 ]; then
    echo
    echo "Individual Time Trial (ITT) processes has exited"
    echo "Look for your log files here: /tmp/solana"
    echo "/tmp/solana/itt-faucet.log" | /usr/games/lolcat -f
    echo "/tmp/solana/itt-bootstrap-validator.log" | /usr/games/lolcat -f
    echo "/tmp/solana/itt-bench-tps.log" | /usr/games/lolcat -f
    echo "Please share/upload them as guided"
    echo
    /usr/bin/toilet -f pagga "Solana is fast"
    echo
    exit 1
  fi
done
