window.BENCHMARK_DATA = {
  "lastUpdate": 1766495988249,
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
      },
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
          "id": "fb5913af9b070904db39523294a9a7e34aa671cc",
          "message": "Benches",
          "timestamp": "2025-12-22T08:36:16Z",
          "url": "https://github.com/fedemagnani/veloce/pull/1/commits/fb5913af9b070904db39523294a9a7e34aa671cc"
        },
        "date": 1766494041781,
        "tool": "cargo",
        "benches": [
          {
            "name": "burst::crossbeam",
            "value": 13229.97,
            "range": "± 121.53",
            "unit": "ns/iter"
          },
          {
            "name": "burst::std_sync",
            "value": 12904.21,
            "range": "± 98.71",
            "unit": "ns/iter"
          },
          {
            "name": "burst::veloce",
            "value": 2021.27,
            "range": "± 13.86",
            "unit": "ns/iter"
          },
          {
            "name": "create::crossbeam",
            "value": 355.97,
            "range": "± 8.78",
            "unit": "ns/iter"
          },
          {
            "name": "create::std_sync",
            "value": 337.68,
            "range": "± 5.15",
            "unit": "ns/iter"
          },
          {
            "name": "create::veloce",
            "value": 56.96,
            "range": "± 0.57",
            "unit": "ns/iter"
          },
          {
            "name": "latency::crossbeam",
            "value": 5194658.85,
            "range": "± 4547573.72",
            "unit": "ns/iter"
          },
          {
            "name": "latency::std_sync",
            "value": 197960982.9,
            "range": "± 33381615.25",
            "unit": "ns/iter"
          },
          {
            "name": "latency::veloce",
            "value": 4016441.4,
            "range": "± 127908.53",
            "unit": "ns/iter"
          },
          {
            "name": "oneshot::crossbeam",
            "value": 395.16,
            "range": "± 2.67",
            "unit": "ns/iter"
          },
          {
            "name": "oneshot::std_sync",
            "value": 371.16,
            "range": "± 5.88",
            "unit": "ns/iter"
          },
          {
            "name": "oneshot::veloce",
            "value": 80.3,
            "range": "± 1.7",
            "unit": "ns/iter"
          },
          {
            "name": "seq_inout::crossbeam",
            "value": 25.03,
            "range": "± 0.37",
            "unit": "ns/iter"
          },
          {
            "name": "seq_inout::std_sync",
            "value": 21.71,
            "range": "± 0.21",
            "unit": "ns/iter"
          },
          {
            "name": "seq_inout::veloce",
            "value": 2.09,
            "range": "± 0.06",
            "unit": "ns/iter"
          },
          {
            "name": "small_buffer::crossbeam",
            "value": 17594017.8,
            "range": "± 3276695.63",
            "unit": "ns/iter"
          },
          {
            "name": "small_buffer::std_sync",
            "value": 10322650.3,
            "range": "± 25066181.8",
            "unit": "ns/iter"
          },
          {
            "name": "small_buffer::veloce_spin",
            "value": 514095.3,
            "range": "± 16936.06",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::crossbeam",
            "value": 14127657.4,
            "range": "± 5107959.97",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::std_sync",
            "value": 3636190,
            "range": "± 2333647.21",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::veloce_spin",
            "value": 483862.32,
            "range": "± 30899.73",
            "unit": "ns/iter"
          },
          {
            "name": "throughput::veloce_try",
            "value": 431562.22,
            "range": "± 23849.48",
            "unit": "ns/iter"
          }
        ]
      }
    ],
    "spsc": [
      {
        "commit": {
          "author": {
            "email": "83358457+fedemagnani@users.noreply.github.com",
            "name": "Federico Magnani",
            "username": "fedemagnani"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5b1ef83118fdc12bace6f4e7658d2a134ca24100",
          "message": "Merge pull request #1 from fedemagnani/benches\n\nBenches",
          "timestamp": "2025-12-23T13:17:33Z",
          "tree_id": "4bfeb87495c9066aa15c0f444a29fc4dfabba91d",
          "url": "https://github.com/fedemagnani/veloce/commit/5b1ef83118fdc12bace6f4e7658d2a134ca24100"
        },
        "date": 1766495987838,
        "tool": "cargo",
        "benches": [
          {
            "name": "spsc::burst::crossbeam",
            "value": 9245.24,
            "range": "± 86.34",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::burst::std_sync",
            "value": 7819.8,
            "range": "± 61.42",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::burst::veloce",
            "value": 951.03,
            "range": "± 4.81",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::create::crossbeam",
            "value": 293.35,
            "range": "± 6.22",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::create::std_sync",
            "value": 287.98,
            "range": "± 5.31",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::create::veloce",
            "value": 46.66,
            "range": "± 0.44",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::latency::crossbeam",
            "value": 2605268.8,
            "range": "± 48612.16",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::latency::std_sync",
            "value": 220108724.8,
            "range": "± 16023863.12",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::latency::veloce",
            "value": 1746007.7,
            "range": "± 42433.12",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::oneshot::crossbeam",
            "value": 314.75,
            "range": "± 3.48",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::oneshot::std_sync",
            "value": 305.08,
            "range": "± 3.75",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::oneshot::veloce",
            "value": 54.26,
            "range": "± 0.97",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::seq_inout::crossbeam",
            "value": 17.42,
            "range": "± 0.13",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::seq_inout::std_sync",
            "value": 15.24,
            "range": "± 0.10",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::seq_inout::veloce",
            "value": 1.55,
            "range": "± 0.01",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::small_buffer::crossbeam",
            "value": 6745229.75,
            "range": "± 1383994.56",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::small_buffer::std_sync",
            "value": 7199939.1,
            "range": "± 13389515.20",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::small_buffer::veloce_spin",
            "value": 845656.32,
            "range": "± 55699.19",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::throughput::crossbeam",
            "value": 4079817.7,
            "range": "± 618933.19",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::throughput::std_sync",
            "value": 1355796.96,
            "range": "± 50670.74",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::throughput::veloce_spin",
            "value": 902397.07,
            "range": "± 35096.98",
            "unit": "ns/iter"
          },
          {
            "name": "spsc::throughput::veloce_try",
            "value": 872632.7,
            "range": "± 13986.64",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}