# tsv2json.py NAME[:TYPE] ... - turn tab-separated records into JSON Lines.
#
# One argument per column, in order. TYPE is str (default), int, or
# cents (a money amount such as 1,440.00 written as the integer 144000,
# so no float rounding ever reaches the JSON).
import json
from decimal import Decimal

import pipelines as p

cols = [(a.split(":") + ["str"])[:2] for a in p.args]
for rec in p.records():
    values = rec.decode("utf-8", "replace").split("\t")
    obj = {}
    for (name, kind), raw in zip(cols, values):
        raw = raw.strip()
        if kind == "int":
            obj[name] = int(raw.replace(",", ""))
        elif kind == "cents":
            obj[name] = int(Decimal(raw.replace(",", "")) * 100)
        else:
            obj[name] = raw
    p.output(json.dumps(obj, ensure_ascii=False))
