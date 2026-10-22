# Count words and write the N most common with their counts (Python stage example).
import re
from collections import Counter
import pipelines as p

n = int(p.args[0]) if p.args else 10
counts = Counter()
for rec in p.records():
    text = rec.decode("utf-8", "replace") if isinstance(rec, bytes) else rec
    counts.update(w for w in re.findall(r"[a-z']+", text.lower()) if len(w) > 3)
for word, c in counts.most_common(n):
    p.output(f"{c:>6} {word}")
