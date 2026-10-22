# Write each line of a web page as a record (Python stage example).
try:
    import truststore
    truststore.inject_into_ssl()
except ImportError:
    pass
import urllib.request
import pipelines as p

with urllib.request.urlopen(p.args[0]) as r:
    for line in r.read().decode("utf-8", "replace").splitlines():
        p.output(line)
