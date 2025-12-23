window.BENCHMARK_DATA = {
  "lastUpdate": 1766493693192,
  "repoUrl": "https://github.com/fedemagnani/veloce",
  "entries": {
    "Rust std::bench": [
      {
        "commit": {
          "author": {
            "name": "fedemagnani",
            "username": "fedemagnani"
          },
          "committer": {
            "name": "fedemagnani",
            "username": "fedemagnani"
          },
          "id": "7a7eaf21c9b3d09b89ce054b12525bd8cd5483ca",
          "message": "Benches",
          "timestamp": "2025-12-22T08:36:16Z",
          "url": "https://github.com/fedemagnani/veloce/pull/1/commits/7a7eaf21c9b3d09b89ce054b12525bd8cd5483ca"
        },
        "date": 1766493692813,
        "tool": "cargo",
        "benches": [
          {
            "name": "burst::crossbeam",
            "value": 8774.53,
            "range": "± 56.83",
            "unit": "ns/iter"
          },
          {
            "name": "burst::std_sync",
            "value": 7507.06,
            "range": "± 34.84",
            "unit": "ns/iter"
          },
          {
            "name": "burst::veloce",
            "value": 948.57,
            "range": "± 8.41",
            "unit": "ns/iter"
          },
          {
            "name": "create::crossbeam",
            "value": 286.02,
            "range": "± 3.71",
            "unit": "ns/iter"
          },
          {
            "name": "create::std_sync",
            "value": 279.9,
            "range": "± 3.27",
            "unit": "ns/iter"
          },
          {
            "name": "create::veloce",
            "value": 46.95,
            "range": "± 0.22",
            "unit": "ns/iter"
          },
          {
            "name": "latency::crossbeam",
            "value": 2666591.9,
            "range": "± 89984.43",
            "unit": "ns/iter"
          },
          {
            "name": "latency::std_sync",
            "value": 210137790.4,
            "range": "± 15978415.62",
            "unit": "ns/iter"
          },
          {
            "name": "latency::veloce",
            "value": 1855263,
            "range": "± 36998.71",
            "unit": "ns/iter"
          },
          {
            "name": "oneshot::crossbeam",
            "value": 304.55,
            "range": "± 3.14",
            "unit": "ns/iter"
          },
          {
            "name": "oneshot::std_sync",
            "value": 294.44,
            "range": "± 5.96",
            "unit": "ns/iter"
          },
          {
            "name": "oneshot::veloce",
            "value": 54.52,
            "range": "± 0.99",
            "unit": "ns/iter"
          },
          {
            "name": "seq_inout::crossbeam",
            "value": 16.8,
            "range": "± 0.20",
            "unit": "ns/iter"
          },
          {
            "name": "seq_inout::std_sync",
            "value": 14.94,
            "range": "± 0.11",
            "unit": "ns/iter"
          },
          {
            "name": "seq_inout::veloce",
            "value": 1.55,
            "range": "± 0.01",
            "unit": "ns/iter"
          },
          {
            "name": "small_buffer::crossbeam",
            "value": 7162100.1,
            "range": "± 250208.94",
            "unit": "ns/iter"
          },
          {
            "name": "small_buffer::std_sync",
            "value": 8295752.5,
            "range": "± 23222966.41",
            "unit": "ns/iter"
          },
          {
            "name": "small_buffer::veloce_spin",
            "value": 1101316.9,
            "range": "± 51253.67",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::crossbeam",
            "value": 5279055.15,
            "range": "± 758931.08",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::std_sync",
            "value": 1336431.48,
            "range": "± 22188.46",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::veloce_spin",
            "value": 1055097.25,
            "range": "± 52564.61",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::veloce_try",
            "value": 1153289.64,
            "range": "± 54932.04",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}