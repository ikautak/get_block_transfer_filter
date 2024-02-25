#!/bin/bash

curl http://127.0.0.1:4000 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0","id":1,
    "method":"getBlock",
    "params": [
      249946646,
      {
        "encoding": "json",
        "maxSupportedTransactionVersion": 0,
        "transactionDetails": "accounts",
        "rewards": false
      }
    ]
  }
'
