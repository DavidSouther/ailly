---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: null
  model: anthropic.claude-3-sonnet-20240229-v1:0
  usage: null
isolated: true
command-line: ailly 20_clock.md --template-view ./templates.yaml
prompt: |
  {{tools.clock.description}}

  Get the current time and tell me what part of the day it is.
---

Sure, here's a Python function that gets the current time and tells you what part of the day it is:

```python
import datetime

def get_time_and_period():
    now = datetime.datetime.now()
    hour = now.hour

    if hour < 12:
        period = "morning"
    elif hour < 18:
        period = "afternoon"
    else:
        period = "evening"

    return f"The current time is {now.strftime('%I:%M %p')} ({period})"

print(get_time_and_period())
```

This function uses the `datetime` module in Python to get the current time and hour of the day. It then checks the hour value to determine whether it's morning, afternoon, or evening.

- If the hour is less than 12, it's considered morning.
- If the hour is between 12 and 18 (6 PM), it's considered afternoon.
- If the hour is 18 or greater, it's considered evening.

The function returns a formatted string with the current time in 12-hour format (e.g., 3:45 PM) and the corresponding period of the day (morning, afternoon, or evening).

When you run this code, it will print the current time and the part of the day it is. For example:

```
The current time is 10:25 AM (morning)
```

Note that this code assumes the following time ranges for each period:

- Morning: 12:00 AM to 11:59 AM
- Afternoon: 12:00 PM to 5:59 PM
- Evening: 6:00 PM to 11:59 PM

You can adjust the time ranges if needed to fit your specific requirements.
