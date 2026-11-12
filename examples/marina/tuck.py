# tuck.py FIELD — store a matched record inside the record it belongs to.
#
# lookup writes two records for each match: the record you looked up
# (a vessel, here), then the record it matched (an owner, or a berth).
# This stage reads those pairs. It stores the second object on the first
# under the name you pass, such as owner or berth.
#
# The join key is that name plus _id, so owner uses owner_id and berth
# uses berth_id. The two records must carry the same key. If they do not,
# the streams were not paired and this stage stops. The error names both
# records and the keys they actually have, so a LOOKUP typo such as
# $owner_i is easier to see.
import json

import pipelines as p

if len(p.args) != 1:
    raise RuntimeError(
        "tuck.py needs the field name to store the matched record under"
    )

field = p.args[0]
key = field + "_id"
pending = None


def describe(obj):
    if not isinstance(obj, dict):
        return "not a JSON object: " + repr(obj)
    keys = ", ".join(obj.keys()) if obj else "no fields"
    if key in obj:
        return key + "=" + repr(obj.get(key)) + "; fields: " + keys
    return "no " + key + "; fields: " + keys


for rec in p.records():
    obj = json.loads(rec.decode("utf-8"))
    if pending is None:
        pending = obj
        continue
    detail = pending
    master = obj
    pending = None
    same = detail.get(key) == master.get(key) and key in detail and key in master
    if not same:
        raise RuntimeError(
            "tuck.py "
            + field
            + ": expected both records to share "
            + key
            + ". First record has "
            + describe(detail)
            + ". Second record has "
            + describe(master)
            + "."
        )
    detail[field] = master
    p.output(json.dumps(detail, ensure_ascii=False, separators=(",", ":")))

if pending is not None:
    raise RuntimeError(
        "tuck.py "
        + field
        + ": the last record had no matching "
        + key
        + " record after it. That record has "
        + describe(pending)
        + "."
    )
