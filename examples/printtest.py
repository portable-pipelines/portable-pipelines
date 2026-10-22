# print() goes to the terminal (stderr); only output() writes records.
import pipelines as p

print("debug: starting")
p.output("record one")
print("debug: done")
