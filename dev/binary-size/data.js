window.BENCHMARK_DATA = {
  "lastUpdate": 1788081775301,
  "repoUrl": "https://github.com/oonid/whatsapp-rust-sqlx",
  "entries": {
    "whatsapp-rust binary size": [
      {
        "commit": {
          "author": {
            "email": "55464917+jlucaso1@users.noreply.github.com",
            "name": "João Lucas",
            "username": "jlucaso1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "302d4787b4b31dd5d1159422741358d4f6f90510",
          "message": "fix(atomics): use portable_atomic for 64-bit atomics + lint against std (#913)",
          "timestamp": "2026-06-22T18:42:27-03:00",
          "tree_id": "2506505bb4a0d96a54aaf5135460060fc18d93af",
          "url": "https://github.com/oonid/whatsapp-rust-sqlx/commit/302d4787b4b31dd5d1159422741358d4f6f90510"
        },
        "date": 1782199085703,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bin size (stripped)",
            "value": 10561336,
            "unit": "bytes"
          },
          {
            "name": "bin .text",
            "value": 8533110,
            "unit": "bytes"
          },
          {
            "name": "bin allocated (text+data+bss)",
            "value": 10559965,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust",
            "value": 1543787,
            "unit": "bytes"
          },
          {
            "name": ".text wacore",
            "value": 536309,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_binary",
            "value": 159587,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_libsignal",
            "value": 169999,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_appstate",
            "value": 147222,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_noise",
            "value": 28375,
            "unit": "bytes"
          },
          {
            "name": ".text waproto",
            "value": 895617,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust_sqlite_storage",
            "value": 473146,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust_tokio_transport",
            "value": 44433,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust_ureq_http_client",
            "value": 9022,
            "unit": "bytes"
          },
          {
            "name": ".text std",
            "value": 1023470,
            "unit": "bytes"
          },
          {
            "name": ".text other deps",
            "value": 3447898,
            "unit": "bytes"
          },
          {
            "name": "llvm-lines wacore",
            "value": 638569,
            "unit": "lines"
          },
          {
            "name": "llvm-lines wacore copies",
            "value": 17724,
            "unit": "copies"
          },
          {
            "name": "llvm-lines whatsapp-rust lib",
            "value": 652550,
            "unit": "lines"
          },
          {
            "name": "llvm-lines whatsapp-rust lib copies",
            "value": 20326,
            "unit": "copies"
          },
          {
            "name": "deps crates (Cargo.lock)",
            "value": 347,
            "unit": "crates"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "55464917+jlucaso1@users.noreply.github.com",
            "name": "João Lucas",
            "username": "jlucaso1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "535732f44a540a7017942a0481eba0a675f2bd0c",
          "message": "fix(send): resend to the devices the fan-out named but never reached (#1363)",
          "timestamp": "2026-08-29T22:02:42-03:00",
          "tree_id": "4d24d26ba0e3a90277919d48500f706dedcf5990",
          "url": "https://github.com/oonid/whatsapp-rust-sqlx/commit/535732f44a540a7017942a0481eba0a675f2bd0c"
        },
        "date": 1788081773159,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bin size (stripped)",
            "value": 10802872,
            "unit": "bytes"
          },
          {
            "name": "bin .text",
            "value": 8678198,
            "unit": "bytes"
          },
          {
            "name": "bin allocated (text+data+bss)",
            "value": 10801318,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust",
            "value": 2036135,
            "unit": "bytes"
          },
          {
            "name": ".text wacore",
            "value": 741842,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_binary",
            "value": 81935,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_libsignal",
            "value": 186852,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_appstate",
            "value": 24582,
            "unit": "bytes"
          },
          {
            "name": ".text wacore_noise",
            "value": 21427,
            "unit": "bytes"
          },
          {
            "name": ".text waproto",
            "value": 1878965,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust_sqlite_storage",
            "value": 563731,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust_tokio_transport",
            "value": 41540,
            "unit": "bytes"
          },
          {
            "name": ".text whatsapp_rust_ureq_http_client",
            "value": 13058,
            "unit": "bytes"
          },
          {
            "name": ".text std",
            "value": 1043447,
            "unit": "bytes"
          },
          {
            "name": ".text other deps",
            "value": 2001781,
            "unit": "bytes"
          },
          {
            "name": "llvm-lines wacore",
            "value": 560164,
            "unit": "lines"
          },
          {
            "name": "llvm-lines wacore copies",
            "value": 18326,
            "unit": "copies"
          },
          {
            "name": "llvm-lines whatsapp-rust lib",
            "value": 799461,
            "unit": "lines"
          },
          {
            "name": "llvm-lines whatsapp-rust lib copies",
            "value": 24827,
            "unit": "copies"
          },
          {
            "name": "deps crates (Cargo.lock)",
            "value": 468,
            "unit": "crates"
          }
        ]
      }
    ]
  }
}