#!/bin/bash
exec env PATINA_AI_INTERFACE=claude patina ai session new --json --interface claude "$@"
