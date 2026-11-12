# Cedar Cove harbor

The office at Cedar Cove keeps four small tables. One lists the people who keep a boat here, including the Harbor Friends Co-op. One lists the slips, including a guest slip that is empty this week. One lists the boats. One lists the work the crew has opened.

A table is easy to correct. When someone asks about a single boat, the office wants one document for that boat: the boat itself, the owner, the slip, and the work orders that belong to it. This folder builds those nested documents from the four tables, and it writes the tables again from the nested documents.

Mara Ellison owns two boats, the sailboat Kestrel and the dinghy Peapod. The co-op owns the skiff Friendship, and Friendship has no work orders open. The guest slip is real, and no boat is in it.

## The files

- `owners.csv` — owner id, name, phone, and email
- `berths.csv` — berth id, dock, slip, and length in feet. Berth `B6` is the empty guest slip on the Guest dock.
- `vessels.csv` — vessel id, name, type, berth id, and owner id
- `work_orders.csv` — work order id, vessel id, the day it was opened, a status, and a short summary. One summary contains a comma.
- `vessels.jsonl` — five nested documents, one per line. The first pipeline writes this same text, and the second pipeline reads this file, so you can run the way back on its own.
- `to-nested.pipe` and `to-csv.pipe` — the two pipelines
- `tuck.py` and `pack.py` — two short Python stages. The built-in stages do the matching. These two place a matched record inside the record it belongs to.
- `to-nested.sh` / `to-nested.ps1` and `to-csv.sh` / `to-csv.ps1` — runners. They look for `pipe` next to the examples, on your PATH, under `target/`, and finally through `cargo`. They create `examples/marina/out/`.

`length_ft` is a number. A phone number stays text, because of the hyphens. A slip name such as `N12` stays text.

## Words for the paths

A stage can have more than one output. The first output is the primary output. The next output is the secondary output.

`collate` joins two streams that are already in order. The parent stream is the master. The child stream is the detail. In this folder the vessels are the master and the work orders are the detail.

A label such as `v:` names a stage. A later path that starts with `v:` connects to that same stage. `(ENDCHAR ?)` means a question mark ends one path, and the next path starts after it.

## What you need

Python 3.10 or later, for `tuck.py` and `pack.py`. The way back uses only built-in stages, so it does not need Python.

Run the pasted commands from the folder that contains `pipe` (the repository root, if you built from source). The scripts may be started from any current directory. They write `examples/marina/out/`. That folder is generated.

On Linux and macOS, and in PowerShell, put the pipeline in single quotes so `$owner_id` reaches `pipe` as those characters. Command Prompt uses double quotes, and `$` is already literal there. The pipelines below use forward slashes in paths. Command Prompt accepts those paths.

## Tables to nested documents

The script writes `examples/marina/out/vessels.jsonl`.

Linux and macOS:

```text
sh ./examples/marina/to-nested.sh
```

PowerShell:

```text
powershell -ExecutionPolicy Bypass -File .\examples\marina\to-nested.ps1
```

The same pipeline, pasted. Create the output folder first if you are not using the script (`mkdir examples/marina/out` or `mkdir examples\marina\out`).

Command Prompt, from the folder that contains `pipe.exe`:

```text
pipe.exe "(ENDCHAR ?) < examples/marina/vessels.csv | csv | v: lookup $owner_id | examples/marina/tuck.py owner | b: lookup $berth_id | examples/marina/tuck.py berth | sort $vessel_id | c: collate $vessel_id | g: fanin | examples/marina/pack.py work_orders work_order_id | sort $vessel_id | > examples/marina/out/vessels.jsonl ? < examples/marina/owners.csv | csv | v: ? < examples/marina/berths.csv | csv | b: ? < examples/marina/work_orders.csv | csv | sort $vessel_id $work_order_id | c: | elastic | g:"
```

Linux and macOS, single quotes:

```text
./pipe '(ENDCHAR ?) < examples/marina/vessels.csv | csv | v: lookup $owner_id | examples/marina/tuck.py owner | b: lookup $berth_id | examples/marina/tuck.py berth | sort $vessel_id | c: collate $vessel_id | g: fanin | examples/marina/pack.py work_orders work_order_id | sort $vessel_id | > examples/marina/out/vessels.jsonl ? < examples/marina/owners.csv | csv | v: ? < examples/marina/berths.csv | csv | b: ? < examples/marina/work_orders.csv | csv | sort $vessel_id $work_order_id | c: | elastic | g:'
```

PowerShell is the same pipeline in single quotes, started with `.\pipe.exe` instead of `./pipe`.

To print the documents instead of writing the file, use `console` in place of `> examples/marina/out/vessels.jsonl`.

### What the pipeline does

`csv` reads a table. The first row names the fields, and each later row becomes one JSON object.

`lookup $owner_id` matches each vessel to an owner. The vessels arrive on the first input. The owners arrive on the second input, which is the `v:` path that reads `owners.csv`. `lookup` reads every owner before it looks at a vessel. For each match it writes the vessel, then the owner, as two records.

There is no built-in stage that places the second of those records inside the first. `tuck.py owner` reads the pair and stores the owner object on the vessel under the name `owner`. The two records must carry the same `owner_id`. A second `lookup`, on `berth_id`, and `tuck.py berth` store the slip under the name `berth`.

If you mistype the LOOKUP field, for example `$owner_i` instead of `$owner_id`, LOOKUP still runs: a missing field is an empty key, so every vessel can pair with the first owner. `tuck.py` then stops because the two records do not share `owner_id`. LOOKUP warns first:

```text
LOOKUP (stage 3): field $owner_i not found in input record; it has owner_id, name, berth_id, ...
```

That warning is on LOOKUP, not only on `tuck.py`. In the GUI the LOOKUP box is marked, Records and Console show the same line, and Source and the Properties operands line squiggle `$owner_i` (`no field owner_i in the input; did you mean owner_id?`). The `tuck.py` error names both records and the keys they actually have.

`sort $vessel_id` lines the vessels up by vessel id. `collate` needs the master stream and the detail stream in the same order. The work orders are sorted by vessel id and then by work order id, so Kestrel's two jobs stay in order.

`collate $vessel_id` walks the two streams together. When a vessel and a work order share a vessel id, it writes the vessel and then each matching work order on the primary output. Friendship has no work order. That vessel leaves on the secondary output.

`fanin` reads its first input to the end, and only then reads its second input. The matched groups are the first input. The vessels with no work orders are the second input. `elastic` holds those set-aside vessels until `fanin` is ready for them, so the two paths have a waiting room and do not get stuck.

`pack.py work_orders work_order_id` reads that combined stream. A record with `work_order_id` is a work order. Any other record is a vessel. The work orders that follow a vessel are stored on it as an array named `work_orders`. Each work order keeps its own vessel id, so the array can be split back into the work order table later. Friendship is followed by no work orders, so its array is empty.

The vessels with no work orders come back at the end of `fanin`. A final `sort $vessel_id` puts the documents in vessel id order, and `>` writes the file. One document per line.

`vessels.jsonl` in this folder is that file. Kestrel looks like this when the line is broken up so you can read it. The pipeline writes it as one line, with no extra spaces.

```json
{
  "vessel_id": "V1",
  "name": "Kestrel",
  "type": "sail",
  "berth_id": "B2",
  "owner_id": "O1",
  "owner": {
    "owner_id": "O1",
    "name": "Mara Ellison",
    "phone": "207-555-0142",
    "email": "mara.ellison@example.com"
  },
  "berth": {
    "berth_id": "B2",
    "dock": "North",
    "slip": "N14",
    "length_ft": 36
  },
  "work_orders": [
    {
      "work_order_id": "W1",
      "vessel_id": "V1",
      "opened": "2026-04-12",
      "status": "open",
      "summary": "Replace the port running light"
    },
    {
      "work_order_id": "W2",
      "vessel_id": "V1",
      "opened": "2026-05-03",
      "status": "done",
      "summary": "Haul, scrape, and paint the bottom"
    }
  ]
}
```

Friendship's `work_orders` array is empty. Peapod's owner is Mara Ellison again, the same person who owns Kestrel.

This command prints each boat with its owner and its slip:

```text
./pipe '< examples/marina/vessels.jsonl | spec $name 1 / (/ next $owner.name next /) at / next $berth.dock next / / next $berth.slip next | console'
```

```text
Kestrel (Mara Ellison) at North N14
Harbor Lark (Luis Ortega) at South S3
Osprey (June Park) at North N12
Friendship (Harbor Friends Co-op) at South S7
Peapod (Mara Ellison) at Rack RA
```

On Command Prompt, use double quotes around that pipeline, and `pipe.exe`.

### One array for one boat, with json collect

`json collect` reads every remaining object and writes one record. The array on that record holds all of them. It is the whole stream, so if you want one boat's list you pick that boat first.

This keeps Kestrel's work orders and packs them into one array:

```text
./pipe '< examples/marina/work_orders.csv | csv | pick $vessel_id == /V1/ | json collect $work_orders | console'
```

```text
{"work_orders":[{"work_order_id":"W1","vessel_id":"V1","opened":"2026-04-12","status":"open","summary":"Replace the port running light"},{"work_order_id":"W2","vessel_id":"V1","opened":"2026-05-03","status":"done","summary":"Haul, scrape, and paint the bottom"}]}
```

That is one array for the rows that were still in the pipe. The harbor pipeline wants a separate array on each vessel, which is why it uses `collate` to keep each vessel with its own work orders, and `pack.py` to store that group.

## Nested documents back to tables

The script reads `examples/marina/vessels.jsonl` and writes four CSV files under `examples/marina/out/`.

Linux and macOS:

```text
sh ./examples/marina/to-csv.sh
```

PowerShell:

```text
powershell -ExecutionPolicy Bypass -File .\examples\marina\to-csv.ps1
```

Command Prompt:

```text
pipe.exe "(ENDCHAR ?) < examples/marina/vessels.jsonl | f: fanout | json keep $vessel_id $name $type $berth_id $owner_id | csv write | > examples/marina/out/vessels.csv ? f: | spec $owner 1 | sort $owner_id | unique | csv write | > examples/marina/out/owners.csv ? f: | spec $berth 1 | sort $berth_id | unique | csv write | > examples/marina/out/berths.csv ? f: | json split $work_orders | sort $work_order_id | csv write | > examples/marina/out/work_orders.csv"
```

Linux and macOS:

```text
./pipe '(ENDCHAR ?) < examples/marina/vessels.jsonl | f: fanout | json keep $vessel_id $name $type $berth_id $owner_id | csv write | > examples/marina/out/vessels.csv ? f: | spec $owner 1 | sort $owner_id | unique | csv write | > examples/marina/out/owners.csv ? f: | spec $berth 1 | sort $berth_id | unique | csv write | > examples/marina/out/berths.csv ? f: | json split $work_orders | sort $work_order_id | csv write | > examples/marina/out/work_orders.csv'
```

PowerShell is that same pipeline in single quotes, started with `.\pipe.exe` instead of `./pipe`.

Create `examples/marina/out` first if you paste the pipeline yourself.

### What the pipeline does

`fanout` copies every nested document down four paths. The label `f:` is that stage. The first path is the primary output. Each later path starts with `f:` again, which attaches the next output of the same `fanout`.

The first path keeps the boat columns with `json keep` and writes `vessels.csv`. The owner id and the berth id are already on the document, beside the nested owner and berth.

The second path takes the owner object. `spec $owner` writes that object as its own record. `sort` and `unique` leave one row for each distinct owner, so Mara Ellison is written once even though she owns two boats. `csv write` writes `owners.csv`. The third path does the same for the berth and writes `berths.csv`.

The fourth path opens the work order array. `json split $work_orders` writes one record for each element. Friendship's array is empty, so that boat adds no row. `sort $work_order_id` orders the jobs, and `csv write` writes `work_orders.csv`. A summary that contains a comma is quoted.

You can print any one of those tables by ending the path at `console` instead of a file. These four commands do that:

```text
./pipe '< examples/marina/vessels.jsonl | json keep $vessel_id $name $type $berth_id $owner_id | csv write | console'
./pipe '< examples/marina/vessels.jsonl | spec $owner 1 | sort $owner_id | unique | csv write | console'
./pipe '< examples/marina/vessels.jsonl | spec $berth 1 | sort $berth_id | unique | csv write | console'
./pipe '< examples/marina/vessels.jsonl | json split $work_orders | sort $work_order_id | csv write | console'
```

To see the open jobs with the boat's name beside the summary, split the array and keep the vessel fields. `keep` on `json split` copies the boat's fields onto each work order, and the work order's own fields stay with it:

```text
./pipe '< examples/marina/vessels.jsonl | json split $work_orders keep | pick $status == /open/ | spec $name 1 /: / next $summary next | console'
```

```text
Kestrel: Replace the port running light
Harbor Lark: Service the diesel injector
Peapod: Varnish the thwart
```

### What comes back

In this sample, `vessels.csv`, `owners.csv`, and `work_orders.csv` come back with the same headers, the same rows, and the same quoting as the files you started from. The work order file is sorted by work order id, which is already the order of the original file.

`berths.csv` comes back without the guest slip `B6`. That slip is in the office's berth table because it is empty. No nested document carries it, so this direction has nothing to write for it. The other slips come back with the same columns, and `length_ft` is a number again.
