export const COMPOSITE_AP = new Set([
    "While Ed juiced the oranges, bread was sliced by Kita.",
    "Three men seized me, and I was carried to the car.",
    "As curators grow a collection, certain works may be chosen to be kept.",
])
export const COMPOSITE_PA = new Set([
    "While the oranges were juiced by Ed, Kita sliced bread.",
    "The scientists are helped by the X process, which uses plasma and radiation to fuse the spherical flask to the testing surface.",
]);
export const COMPOSITE = new Set([
    ...COMPOSITE_AP,
    ...COMPOSITE_PA,
])
export const PASSIVE = new Set([
    "John was accused of committing crimes by David.",
    "She was sent a cheque for a thousand euros.",
    "He was given a book for his birthday.",
    "He will be sent away to school.",
    "The meeting was called off.",
    "He was looked after by his grandmother.",
    "The grizzly was humiliated by the bobcat.",
    "The money was lost by him.",
    "A meal is being cooked by me.",
    "Her dog will be walked.",
    "The sweater was worn by them.",
    "The kite was flown by John.",
    "While the oranges were juiced by Ed, bread was sliced by Kita.",
    "Eventually the theater was destroyed.",
    "The experiment was performed and the results were recorded.",
    "It is believed that this new method is safer.",
    "Butternut squash, a favorite among gourd enthusiasts, can be roasted, grilled, or even fried.",
    "Bananas were bought by me at the store today.",
    "Mark was pushed into the pool by Steve.",
    ...COMPOSITE,
]);
export const ACTIVE = new Set([
    "David accused John of committing crimes.",
    "Someone sent her a cheque for a thousand euros.",
    "I gave him a book for his birthday.",
    "They will send him away to school.",
    "They called off the meeting.",
    "His grandmother looked after him.",
    "The bobcat humiliated the grizzly.",
    "My brother lost the money.",
    "I am cooking a meal.",
    "Someone will walk her dog.",
    "They wore a sweater.",
    "John flew the kite.",
    "Cooks can roast, grill, or even fry butternut squash, a favorite among gourd enthusiasts.",
    "While Ed juiced the oranges, Kita sliced bread.",
    "I bought bananas at the store today.",
    "Eventually a construction crew destroyed the theater.",
    "We performed the experiment and recorded the results.",
    "We believe this new method is safer.",
    "As curators grow a collection, they may choose to keep certain works.",
    "The X process, which uses plasma and radiation to fuse the spherical flask to the testing surface, helps the scientists.",
    "Steve pushed Mark into the pool.",
    ...COMPOSITE,
]);

export const ALL = new Set([
    ...ACTIVE, ...PASSIVE
])
