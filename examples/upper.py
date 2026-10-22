# Uppercase each input record (Python stage example).
import pipelines as p

for rec in p.records():
    p.output(rec.upper())
