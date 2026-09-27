{
  nixos = _: { fixture.account.alice = true; };
  homeManager = _: { fixture.home.alice = true; };
}
