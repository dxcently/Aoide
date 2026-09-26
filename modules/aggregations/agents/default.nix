# agents — the coding agents Aoide conducts.
#
# One membership, no platform half: every member installs its own CLI and
# carries its own credentials story, so there is nothing for the aggregation to
# set. A host that wants a subset drops a member the ordinary way
# (`dendrites.kimi-code.enable = false`) — the group says what a machine of this
# kind usually runs, not what it must.
#
# `melete` and `mneme` are NOT members: they are the servers an agent dispatches
# to over MCP, not the agents themselves, and a host names one only when it hosts
# one.
{
  description = "The coding agents a workstation runs: Claude Code, Eidolon, Kimi, OpenAI and pi.";

  system.members = [
    "claude-code"
    "eidolon"
    "kimi-code"
    "openai"
    "pi-coding-agent"
  ];
}
