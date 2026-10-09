from harness import *
cases = [("fma","tz",F(0,10,0),F(0,10**33,0),F(1,1,-102)), ("add","ne",F(0,1,0),F(1,1,0)), ("add","tn",F(0,1,0),F(1,1,0)), ("sqrt","ne",F(1,0,-3))]
n, bad = compare(cases)
print(n, bad)
