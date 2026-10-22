# Write each stage argument as one record (Python analogue of emit.rexx).
import pipelines as p

for a in p.args:
    p.output(a)
