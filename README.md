## Usage

```
icmp-echo [destination address,number of requests, request interval in milliseconds]

e.g.: cargo run -- 1.1.1.1,30,50
```

Sample response:

```
1.1.1.1,0,54323
1.1.1.1,1,55129
1.1.1.1,2,37229
1.1.1.1,3,35869
1.1.1.1,4,41214
```