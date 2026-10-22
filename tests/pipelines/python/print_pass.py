import pipelines as p

print("debug-should-not-be-a-record")
for rec in p.records():
    p.output(rec)
