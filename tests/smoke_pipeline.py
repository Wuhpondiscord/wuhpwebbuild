"""Exercise the public Phase 1 API against the built application."""

import json
import sys
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def api(path, body=None, status=200):
    data = None if body is None else json.dumps(body).encode()
    request = Request(f"{sys.argv[1]}/api/v1/{path}", data, {
        "Content-Type": "application/json",
    })
    try:
        response = urlopen(request, timeout=10)
    except HTTPError as error:
        response = error
    with response:
        assert response.status == status, (path, response.status, response.read())
        return json.load(response)


samples = api("leads/demo")
assert len(samples) == 3
assert samples[0]["source"] == {"kind": "fixture"}
assert {lead["signal"] for lead in samples} == {
    "no_website_listed", "social_or_directory", "website_listed",
}

imported = api("leads/import", {
    "format": "csv",
    "content": 'name,category,services\n"Shop, LLC",Repair,"Inspection & <testing>"',
})
lead = imported[0]
assert lead["lead"]["name"] == "Shop, LLC"
assert lead["source"] == {"kind": "operator"}
api("leads/import", {"format": "csv", "content": "name,category\n,Repair"}, 422)
osm = api("leads/import", {
    "format": "osm",
    "content": json.dumps({
        "elements": [{"type": "way", "id": 7, "tags": {
            "name": "Car Shop",
            "shop": "car_repair",
            "contact:website": "www.instagram.com/car-shop",
        }}],
    }),
})
assert osm[0]["source"] == {"kind": "osm", "reference": "way/7"}
assert osm[0]["signal"] == "social_or_directory"

candidate = {"lead": lead["lead"], "source": lead["source"], "confirmed": False}
api("brief", candidate, 422)
candidate["confirmed"] = True
brief = api("brief", candidate)
assert "Phone" in brief["unknowns"]
assert any(fact["value"] == "Inspection & <testing>" for fact in brief["facts"])
api("previews", {**candidate, "approved": False}, 422)
variants = api("previews", {**candidate, "approved": True})
assert [variant["id"] for variant in variants] == ["A", "B"]
for variant in variants:
    assert "Concept preview prepared by your agency" in variant["html"]
    assert "Inspection &amp; &lt;testing&gt;" in variant["html"]
    assert "Content-Security-Policy" in variant["html"]
    assert "<testing>" not in variant["html"]
assert variants[0]["html"] != variants[1]["html"]

print("Phase 1 import, confirmation, approval, and preview smoke checks passed.")
