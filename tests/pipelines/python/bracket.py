import pipelines as p

while True:
    rec = p.peekto()
    if rec is None:
        break
    rec = p.readto()
    p.output(b"[" + rec + b"]")
