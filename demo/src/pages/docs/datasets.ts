// Small, self-contained datasets bundled with the docs pages so every live
// editor renders without a network round-trip. The `.ag` sources in
// `content.tsx` are validated against these files with `algraf check`.

export const PENGUINS_CSV = `species,flipper_length_mm,body_mass_g,island
Adelie,181,3750,Torgersen
Adelie,186,3800,Torgersen
Adelie,195,3350,Dream
Adelie,193,3450,Dream
Adelie,190,3650,Biscoe
Chinstrap,196,3550,Dream
Chinstrap,201,3950,Dream
Chinstrap,207,4050,Dream
Chinstrap,210,4100,Dream
Chinstrap,198,3700,Dream
Gentoo,210,4400,Biscoe
Gentoo,215,4850,Biscoe
Gentoo,222,5250,Biscoe
Gentoo,230,5550,Biscoe
Gentoo,218,5100,Biscoe
`;

export const SALES_CSV = `quarter,type,amount
Q1,software,42
Q1,services,24
Q1,hardware,18
Q2,software,47
Q2,services,29
Q2,hardware,16
Q3,software,53
Q3,services,31
Q3,hardware,21
Q4,software,61
Q4,services,36
Q4,hardware,25
`;

export const PROFIT_CSV = `month,profit,status
Jan,12000,Profit
Feb,8000,Profit
Mar,-4000,Loss
Apr,-12000,Loss
May,15000,Profit
Jun,22000,Profit
Jul,-6000,Loss
Aug,-9000,Loss
Sep,18000,Profit
Oct,25000,Profit
Nov,31000,Profit
Dec,42000,Profit
`;

export const FORECAST_CSV = `day,lower,upper,series
2026-01-01,10,14,midpoint
2026-01-02,12,18,midpoint
2026-01-03,13,21,midpoint
2026-01-04,11,19,midpoint
2026-01-05,15,24,midpoint
2026-01-06,17,26,midpoint
`;

export const REGIONS_CSV = `time,region,product,sales
2026-01-01,North,Widgets,150
2026-01-02,North,Widgets,200
2026-01-03,North,Widgets,180
2026-01-01,North,Gadgets,90
2026-01-02,North,Gadgets,110
2026-01-03,North,Gadgets,105
2026-01-01,South,Widgets,210
2026-01-02,South,Widgets,190
2026-01-03,South,Widgets,220
2026-01-01,South,Gadgets,130
2026-01-02,South,Gadgets,140
2026-01-03,South,Gadgets,160
2026-01-01,East,Widgets,120
2026-01-02,East,Widgets,130
2026-01-03,East,Widgets,140
2026-01-01,East,Gadgets,80
2026-01-02,East,Gadgets,90
2026-01-03,East,Gadgets,100
2026-01-01,West,Widgets,170
2026-01-02,West,Widgets,180
2026-01-03,West,Widgets,190
2026-01-01,West,Gadgets,100
2026-01-02,West,Gadgets,110
2026-01-03,West,Gadgets,120
`;

export const HEIGHTS_CSV = `gender,height
Female,165.2
Female,160.0
Female,170.5
Female,168.1
Female,162.4
Male,175.8
Male,180.2
Male,178.0
Male,182.5
Male,174.9
Non-binary,170.0
Non-binary,174.5
Non-binary,168.8
`;

export const PANELS_CSV = `x,y,series,row_band,col_band,label
1,1,A,North,Q1,A1
2,2,A,North,Q2,A2
3,3,A,South,Q1,A3
10,4,A,South,Q2,A4
100,12,B,North,Q1,B1
150,18,B,North,Q2,B2
200,25,B,South,Q1,B3
250,30,B,South,Q2,B4
`;

export const INSET_CITIES_CSV = `city,long,lat,population
New York,-74.006,40.7128,8400000
Los Angeles,-118.2437,34.0522,3900000
Chicago,-87.6298,41.8781,2700000
Houston,-95.3698,29.7604,2300000
Phoenix,-112.074,33.4484,1600000
Seattle,-122.3321,47.6062,750000
Miami,-80.1918,25.7617,470000
Denver,-104.9903,39.7392,715000
`;

export const CITY_MIX_CSV = `city,age_group,count
New York,under 25,2600
New York,25-64,4700
New York,65+,1100
Los Angeles,under 25,1300
Los Angeles,25-64,2100
Los Angeles,65+,500
Chicago,under 25,850
Chicago,25-64,1500
Chicago,65+,350
Houston,under 25,760
Houston,25-64,1240
Houston,65+,300
Phoenix,under 25,470
Phoenix,25-64,900
Phoenix,65+,230
Seattle,under 25,190
Seattle,25-64,470
Seattle,65+,90
Miami,under 25,120
Miami,25-64,260
Miami,65+,90
Denver,under 25,180
Denver,25-64,430
Denver,65+,105
`;

export const DISTRIBUTION_CSV = `value
46.9
56.1
47.3
46.2
38.8
47.4
63.3
55.1
62.4
53.0
54.7
52.2
30.0
60.3
56.1
56.0
29.7
29.1
39.3
44.4
53.7
49.4
56.3
42.3
53.7
54.7
42.1
70.6
56.7
64.4
42.6
41.1
45.9
48.7
57.6
53.0
44.6
38.5
43.8
64.7
40.3
52.9
55.1
32.1
50.6
65.7
25.8
46.1
48.7
40.2
56.0
49.3
32.4
59.9
58.0
61.4
67.3
54.3
51.4
34.4
57.4
42.7
44.6
34.8
38.4
43.6
65.5
25.6
32.5
52.9
`;

export const SCORES_CSV = `score,cohort
84,A
77,A
54,A
49,A
75,A
65,A
61,A
80,A
81,A
73,A
74,A
75,A
86,A
77,A
76,A
76,A
57,A
83,A
80,A
76,A
54,A
66,A
79,A
55,A
70,A
81,A
60,A
86,A
76,A
70,A
74,A
77,A
73,A
82,A
66,A
68,A
81,A
72,A
64,A
80,A
93,B
78,B
70,B
80,B
80,B
79,B
93,B
73,B
92,B
71,B
75,B
87,B
91,B
88,B
84,B
83,B
83,B
86,B
80,B
84,B
86,B
82,B
88,B
86,B
98,B
84,B
78,B
79,B
81,B
89,B
79,B
85,B
96,B
61,B
73,B
83,B
85,B
83,B
78,B
87,B
`;
