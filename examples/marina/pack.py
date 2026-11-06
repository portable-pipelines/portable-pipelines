# pack.py ARRAYFIELD CHILDKEY — pack following child records onto a parent.
#
# collate writes the parent record, then each child that shares its key.
# A later parent starts the next group. This stage reads that stream.
# A record that has CHILDKEY is a child. Any other object is a parent.
# Children that follow a parent are stored on it as a JSON array named
# ARRAYFIELD. A parent with no children gets an empty array.
import json

import pipelines as p

if len(p.args) != 2:
    raise RuntimeError(
        "pack.py needs the array field name and the key that marks a child record"
    )

array_field = p.args[0]
child_key = p.args[1]
current = None
children = []


def emit(parent, kids):
    parent[array_field] = kids
    p.output(json.dumps(parent, ensure_ascii=False, separators=(",", ":")))


for rec in p.records():
    obj = json.loads(rec.decode("utf-8"))
    if child_key in obj:
        if current is None:
            raise RuntimeError(
                "pack.py saw a child record before any parent. "
                "The parent should be written first."
            )
        children.append(obj)
        continue
    if current is not None:
        emit(current, children)
    current = obj
    children = []

if current is not None:
    emit(current, children)
