---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: null
  model: anthropic.claude-3-sonnet-20240229-v1:0
  usage: null
isolated: false
prompt: |
  {{claude.long}}
  {{format.ad_json}}

  Create an an ad spot for this product.
---

{
"campaign_id": "seltzerbrand_001",
"ad_copy": "Behold, a libation of unparalleled effervescence and refined taste, crafted with the utmost care and artistry. Our new line of seltzer beverages promises to elevate your senses and transport you to a realm of exquisite indulgence. Each delicate bubble bursts forth with a tantalizing bouquet of natural flavors, meticulously curated to tantalizing your palate. Embrace the epitome of sophisticated refreshment, a harmonious fusion of invigorating sparkle and sublime essence. Indulge in a sip that transcends the ordinary, and revel in the delightfully effervescent experience that awaits you."
}
