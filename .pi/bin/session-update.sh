#!/bin/bash
exec env PATINA_AI_INTERFACE=pi patina ai session update --json "$@"
