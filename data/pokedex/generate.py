#!/usr/bin/env python3
"""Single source of truth for the pokedex pilot. Regenerates the three
data files, questions.json (expected values cross-checked), and
src/pokedex-reference.yml. Run: python3 data/pokedex/generate.py"""
#!/usr/bin/env python3
"""Generate the three pokedex data files from one master definition, and
compute the expected answers for the questions so they can be cross-checked.
Writes data.sql / data.cypher / data.tql into data/pokedex/<db>/."""
import os, collections

import os.path
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# ---- master data -----------------------------------------------------------
types = [  # id, name
    (1,'Normal'),(2,'Fire'),(3,'Water'),(4,'Electric'),(5,'Grass'),(6,'Ice'),
    (7,'Fighting'),(8,'Poison'),(9,'Ground'),(10,'Flying'),(11,'Psychic'),(12,'Bug'),
    (13,'Rock'),(14,'Ghost'),(15,'Dragon'),(16,'Dark'),(17,'Steel'),(18,'Fairy')]

abilities = [
    (1,'Overgrow'),(2,'Blaze'),(3,'Torrent'),(4,'Static'),(5,'Water Absorb'),
    (6,'Volt Absorb'),(7,'Flash Fire'),(8,'Thick Fat'),(9,'Intimidate'),(10,'Levitate'),
    (11,'Synchronize'),(12,'Guts'),(13,'Run Away'),(14,'Solar Power')]

methods = [(1,'Level up'),(2,'Machine'),(3,'Tutor'),(4,'Egg')]

pokemon = [  # id, name, generation
    (1,'Bulbasaur',1),(2,'Venusaur',1),(3,'Charmander',1),(4,'Charizard',1),
    (5,'Squirtle',1),(6,'Blastoise',1),(7,'Pikachu',1),(8,'Raichu',1),
    (9,'Gyarados',1),(10,'Vaporeon',1),(11,'Jolteon',1),(12,'Flareon',1),
    (13,'Snorlax',1),(14,'Dragonite',1),(15,'Gengar',1),(16,'Alakazam',1),
    (17,'Machamp',1),(18,'Lapras',1),(19,'Eevee',1)]

moves = [  # id, name, power, type_id, damage_class
    (1,'Thunderbolt',90,4,'special'),(2,'Thunder',100,4,'special'),(3,'Thunder Shock',40,4,'special'),
    (4,'Quick Attack',40,1,'physical'),(5,'Surf',90,3,'special'),(6,'Hydro Pump',110,3,'special'),
    (7,'Water Gun',40,3,'special'),(8,'Flamethrower',90,2,'special'),(9,'Fire Blast',120,2,'special'),
    (10,'Ember',40,2,'special'),(11,'Tackle',35,1,'physical'),(12,'Ice Beam',90,6,'special'),
    (13,'Body Slam',85,1,'physical'),(14,'Vine Whip',45,5,'special'),(15,'Razor Leaf',55,5,'physical'),
    (16,'Psychic',90,11,'special'),(17,'Dragon Rush',100,15,'physical'),(18,'Hyper Beam',150,1,'special')]

pk_type = [  # pokemon_id, type_id, slot
    (1,5,1),(1,8,2),(2,5,1),(2,8,2),(3,2,1),(4,2,1),(4,10,2),(5,3,1),(6,3,1),
    (7,4,1),(8,4,1),(9,3,1),(9,10,2),(10,3,1),(11,4,1),(12,2,1),(13,1,1),
    (14,15,1),(14,10,2),(15,14,1),(15,8,2),(16,11,1),(17,7,1),(18,3,1),(18,6,2),(19,1,1)]

pk_ability = [  # pokemon_id, ability_id, is_hidden, slot
    (1,1,False,1),(2,1,False,1),(3,2,False,1),(4,2,False,1),(4,14,True,3),
    (5,3,False,1),(6,3,False,1),(7,4,False,1),(8,4,False,1),(9,9,False,1),
    (10,5,False,1),(11,6,False,1),(12,7,False,1),(13,8,False,1),(14,9,False,1),
    (15,10,False,1),(16,11,False,1),(17,12,False,1),(18,5,False,1),(19,13,False,1)]

pk_move = [  # pokemon_id, move_id, method_id, level
    (7,1,1,36),(7,3,1,1),(7,2,2,0),(7,4,1,10),
    (11,1,1,1),(11,3,1,1),
    (8,1,2,0),(8,2,2,0),
    (5,7,1,1),(5,5,2,0),
    (6,6,1,52),(6,5,2,0),
    (10,7,1,1),(10,5,2,0),
    (9,6,1,40),
    (18,12,1,45),(18,5,2,0),
    (3,10,1,1),(3,8,2,0),
    (4,8,1,30),(4,9,2,0),(4,5,2,0),
    (12,10,1,1),
    (1,14,1,10),(1,15,1,20),
    (2,15,1,20),
    (13,13,1,30),(13,18,2,0),
    (19,4,1,1)]

efficacy = [  # damage_type_id, target_type_id, factor
    (3,2,200),(3,9,200),(3,13,200),(3,3,50),(3,5,50),(3,15,50),
    (4,3,200),(4,10,200),(4,9,0),(4,4,50),(4,5,50),(4,15,50),
    (2,5,200),(2,6,200),(2,12,200),(2,17,200),(2,2,50),(2,3,50),(2,13,50),(2,15,50),
    (5,3,200),(5,9,200),(5,13,200),(5,2,50),(5,5,50)]

TN = {i:n for i,n in types}
AN = {i:n for i,n in abilities}
MN = {i:n for i,n in methods}
PN = {i:n for i,n,_ in pokemon}
MVN = {i:n for i,n,_,_,_ in moves}
MV_TYPE = {i:t for i,_,_,t,_ in moves}
MV_POW = {i:p for i,_,p,_,_ in moves}

def sq(s):  # sql/cypher single-quote escape
    return s.replace("'", "''")

# ---- SQL -------------------------------------------------------------------
def gen_sql():
    L = []
    L.append("INSERT INTO poketype (id, name) VALUES")
    L.append(",\n".join(f"    ({i}, '{sq(n)}')" for i,n in types) + ";\n")
    L.append("INSERT INTO ability (id, name) VALUES")
    L.append(",\n".join(f"    ({i}, '{sq(n)}')" for i,n in abilities) + ";\n")
    L.append("INSERT INTO move_method (id, name) VALUES")
    L.append(",\n".join(f"    ({i}, '{sq(n)}')" for i,n in methods) + ";\n")
    L.append("INSERT INTO pokemon (id, name, generation) VALUES")
    L.append(",\n".join(f"    ({i}, '{sq(n)}', {g})" for i,n,g in pokemon) + ";\n")
    L.append("INSERT INTO move (id, name, power, poketype_id, damage_class) VALUES")
    L.append(",\n".join(f"    ({i}, '{sq(n)}', {p}, {t}, '{c}')" for i,n,p,t,c in moves) + ";\n")
    L.append("INSERT INTO pokemon_poketype (pokemon_id, poketype_id, slot) VALUES")
    L.append(",\n".join(f"    ({a}, {b}, {s})" for a,b,s in pk_type) + ";\n")
    L.append("INSERT INTO pokemon_ability (pokemon_id, ability_id, is_hidden, slot) VALUES")
    L.append(",\n".join(f"    ({a}, {b}, {str(h).lower()}, {s})" for a,b,h,s in pk_ability) + ";\n")
    L.append("INSERT INTO pokemon_move (pokemon_id, move_id, method_id, level) VALUES")
    L.append(",\n".join(f"    ({a}, {b}, {c}, {d})" for a,b,c,d in pk_move) + ";\n")
    L.append("INSERT INTO efficacy (damage_poketype_id, target_poketype_id, factor) VALUES")
    L.append(",\n".join(f"    ({a}, {b}, {f})" for a,b,f in efficacy) + ";\n")
    return "\n".join(L)

# ---- Cypher ----------------------------------------------------------------
def gen_cypher():
    L = ["// Reseed idempotently: wipe first, then load.", "MATCH (n) DETACH DELETE n;", ""]
    for i,n in types: L.append(f"CREATE (:PokeType {{name:'{sq(n)}'}});")
    L.append("")
    for i,n in abilities: L.append(f"CREATE (:Ability {{name:'{sq(n)}'}});")
    L.append("")
    for i,n in methods: L.append(f"CREATE (:MoveMethod {{name:'{sq(n)}'}});")
    L.append("")
    for i,n,g in pokemon: L.append(f"CREATE (:Pokemon {{id:{i}, name:'{sq(n)}', generation:{g}}});")
    L.append("")
    for i,n,p,t,c in moves:
        L.append(f"CREATE (:Move {{id:{i}, name:'{sq(n)}', power:{p}, damage_class:'{c}'}});")
    L.append("")
    for i,n,p,t,c in moves:
        L.append(f"MATCH (m:Move {{name:'{sq(n)}'}}),(t:PokeType {{name:'{sq(TN[t])}'}}) MERGE (m)-[:OF_TYPE]->(t);")
    L.append("")
    for a,b,s in pk_type:
        L.append(f"MATCH (p:Pokemon {{name:'{sq(PN[a])}'}}),(t:PokeType {{name:'{sq(TN[b])}'}}) MERGE (p)-[:HAS_TYPE {{slot:{s}}}]->(t);")
    L.append("")
    for a,b,h,s in pk_ability:
        L.append(f"MATCH (p:Pokemon {{name:'{sq(PN[a])}'}}),(x:Ability {{name:'{sq(AN[b])}'}}) MERGE (p)-[:HAS_ABILITY {{is_hidden:{str(h).lower()}, slot:{s}}}]->(x);")
    L.append("")
    for a,b,f in efficacy:
        L.append(f"MATCH (d:PokeType {{name:'{sq(TN[a])}'}}),(g:PokeType {{name:'{sq(TN[b])}'}}) MERGE (d)-[:EFFECTIVE_AGAINST {{factor:{f}}}]->(g);")
    L.append("")
    L.append("// learning reified as a node linking pokemon, move and method")
    for a,b,c,lvl in pk_move:
        L.append(f"MATCH (p:Pokemon {{name:'{sq(PN[a])}'}}),(m:Move {{name:'{sq(MVN[b])}'}}),(mm:MoveMethod {{name:'{sq(MN[c])}'}}) CREATE (p)-[:LEARNS]->(l:Learning {{level:{lvl}}})-[:OF_MOVE]->(m) CREATE (l)-[:VIA]->(mm);")
    return "\n".join(L) + "\n"

# ---- TypeQL ----------------------------------------------------------------
def var(prefix, name):
    return "$" + prefix + name.lower().replace(" ", "_").replace("-", "_")

def gen_typeql():
    L = ["insert"]
    for i,n in types: L.append(f'  {var("t_",n)} isa poketype, has name "{n}";')
    for i,n in abilities: L.append(f'  {var("a_",n)} isa ability, has name "{n}";')
    for i,n in methods: L.append(f'  {var("mm_",n)} isa move-method, has name "{n}";')
    for i,n,g in pokemon: L.append(f'  {var("p_",n)} isa pokemon, has name "{n}", has generation {g};')
    for i,n,p,t,c in moves:
        L.append(f'  {var("m_",n)} isa move, has name "{n}", has power {p}, has damage-class "{c}";')
    L.append("  # move -> type")
    for i,n,p,t,c in moves:
        L.append(f'  (typed: {var("m_",n)}, category: {var("t_",TN[t])}) isa move-type;')
    L.append("  # pokemon -> type")
    for a,b,s in pk_type:
        L.append(f'  (bearer: {var("p_",PN[a])}, category: {var("t_",TN[b])}) isa typing, has slot {s};')
    L.append("  # pokemon -> ability")
    for a,b,h,s in pk_ability:
        L.append(f'  (owner: {var("p_",PN[a])}, granted: {var("a_",AN[b])}) isa ability-slot, has is-hidden {str(h).lower()}, has slot {s};')
    L.append("  # type efficacy")
    for a,b,f in efficacy:
        L.append(f'  (attacker: {var("t_",TN[a])}, defender: {var("t_",TN[b])}) isa efficacy, has factor {f};')
    L.append("  # learning (ternary)")
    for a,b,c,lvl in pk_move:
        L.append(f'  (learner: {var("p_",PN[a])}, learned: {var("m_",MVN[b])}, via: {var("mm_",MN[c])}) isa learning, has level {lvl};')
    return "\n".join(L) + "\n"

# ---- expected-answer cross-check -------------------------------------------
def compute_expected():
    pk_types_by_name = collections.defaultdict(set)   # pokemon name -> set(type names)
    primary = {}                                       # pokemon name -> slot1 type name
    for a,b,s in pk_type:
        pk_types_by_name[PN[a]].add(TN[b])
        if s == 1: primary[PN[a]] = TN[b]
    e = {}
    # Q1 fire count
    e['Q1'] = sum(1 for p in PN.values() if 'Fire' in pk_types_by_name[p])
    # Q2 charizard primary
    e['Q2'] = primary['Charizard']
    # Q3 avg power water moves
    wp = [MV_POW[i] for i in MVN if MV_TYPE[i]==3]
    e['Q3'] = sum(wp)/len(wp)
    # Q4 any water+flying
    e['Q4'] = any({'Water','Flying'} <= pk_types_by_name[p] for p in PN.values())
    # Q5 grass pokemon
    e['Q5'] = sorted(p for p in PN.values() if 'Grass' in pk_types_by_name[p])
    # Q6 electric moves name+power
    e['Q6'] = sorted(([MVN[i], MV_POW[i]] for i in MVN if MV_TYPE[i]==4))
    # Q7 top-3 power desc
    e['Q7'] = [MVN[i] for i in sorted(MVN, key=lambda i:-MV_POW[i])[:3]]
    # Q8 pokemon that learn an electric move (type 4)
    elec_move_ids = {i for i in MVN if MV_TYPE[i]==4}
    e['Q8'] = sorted({PN[a] for a,b,c,l in pk_move if b in elec_move_ids})
    # Q9 thunderbolt (id1) by level up (method1)
    e['Q9'] = sorted(([PN[a], l] for a,b,c,l in pk_move if b==1 and c==1))
    # Q10 water super-effective (>100)
    e['Q10'] = sorted(TN[b] for a,b,f in efficacy if a==3 and f>100)
    # Q11 static holders (ability 4)
    e['Q11'] = sorted(PN[a] for a,b,h,s in pk_ability if b==4)
    # Q12 fire types that cannot learn a water move (type 3)
    water_move_ids = {i for i in MVN if MV_TYPE[i]==3}
    can_water = {PN[a] for a,b,c,l in pk_move if b in water_move_ids}
    fire_pk = {p for p in PN.values() if 'Fire' in pk_types_by_name[p]}
    e['Q12'] = sorted(fire_pk - can_water)
    return e


# ---- write data files ------------------------------------------------------
open(f"{ROOT}/data/pokedex/sql/data.sql","w").write(gen_sql())
open(f"{ROOT}/data/pokedex/neo4j/data.cypher","w").write(gen_cypher())
open(f"{ROOT}/data/pokedex/typedb/data.tql","w").write(gen_typeql())
EXPECTED = compute_expected()

# ---- questions.json --------------------------------------------------------
import json

Q4_TYPEQL = """with fun water_flyer_exists() -> boolean:
  match
    $p isa pokemon;
    $w isa poketype, has name "Water";
    $f isa poketype, has name "Flying";
    typing (bearer: $p, category: $w);
    typing (bearer: $p, category: $f);
  return check;
match
let $exists = water_flyer_exists();"""

questions = [
 {"question":"How many Pokémon have the Fire type?","difficulty":"easy","expected":3,
  "queries":{
   "typeql":'match $fire isa poketype, has name "Fire"; typing (bearer: $p, category: $fire); reduce $count = count($p);',
   "sql":"SELECT COUNT(*) FROM pokemon p JOIN pokemon_poketype pt ON pt.pokemon_id = p.id JOIN poketype t ON pt.poketype_id = t.id WHERE t.name = 'Fire';",
   "cypher":"MATCH (p:Pokemon)-[:HAS_TYPE]->(:PokeType {name: 'Fire'}) RETURN count(p)"}},

 {"question":"What is Charizard's primary type?","difficulty":"medium","expected":"Fire",
  "queries":{
   "typeql":'match $p isa pokemon, has name "Charizard"; $tp isa typing, links (bearer: $p, category: $t), has slot 1; $t has name $n; select $n;',
   "sql":"SELECT t.name FROM pokemon p JOIN pokemon_poketype pt ON pt.pokemon_id = p.id JOIN poketype t ON pt.poketype_id = t.id WHERE p.name = 'Charizard' AND pt.slot = 1;",
   "cypher":"MATCH (:Pokemon {name: 'Charizard'})-[:HAS_TYPE {slot: 1}]->(t:PokeType) RETURN t.name"}},

 {"question":"What is the average power of Water-type moves?","difficulty":"medium","expected":80.0,
  "queries":{
   "typeql":'match $water isa poketype, has name "Water"; move-type (typed: $m, category: $water); $m has power $pw; reduce $avg = mean($pw);',
   "sql":"SELECT AVG(m.power) FROM move m JOIN poketype t ON m.poketype_id = t.id WHERE t.name = 'Water';",
   "cypher":"MATCH (m:Move)-[:OF_TYPE]->(:PokeType {name: 'Water'}) RETURN avg(m.power)"}},

 {"question":"Is there a Pokémon that is both Water and Flying type?","difficulty":"medium","expected":True,
  "queries":{
   "typeql":Q4_TYPEQL,
   "sql":"SELECT EXISTS (SELECT 1 FROM pokemon p JOIN pokemon_poketype pt1 ON pt1.pokemon_id = p.id JOIN poketype t1 ON pt1.poketype_id = t1.id JOIN pokemon_poketype pt2 ON pt2.pokemon_id = p.id JOIN poketype t2 ON pt2.poketype_id = t2.id WHERE t1.name = 'Water' AND t2.name = 'Flying');",
   "cypher":"RETURN EXISTS { MATCH (p:Pokemon)-[:HAS_TYPE]->(:PokeType {name: 'Water'}), (p)-[:HAS_TYPE]->(:PokeType {name: 'Flying'}) }"}},

 {"question":"List the names of all Grass-type Pokémon.","difficulty":"easy","expected":["Bulbasaur","Venusaur"],
  "queries":{
   "typeql":'match $grass isa poketype, has name "Grass"; typing (bearer: $p, category: $grass); $p has name $n; select $n;',
   "sql":"SELECT p.name FROM pokemon p JOIN pokemon_poketype pt ON pt.pokemon_id = p.id JOIN poketype t ON pt.poketype_id = t.id WHERE t.name = 'Grass';",
   "cypher":"MATCH (p:Pokemon)-[:HAS_TYPE]->(:PokeType {name: 'Grass'}) RETURN p.name"}},

 {"question":"List the name and power of each Electric-type move.","difficulty":"medium",
  "expected":[{"move":"Thunderbolt","power":90},{"move":"Thunder","power":100},{"move":"Thunder Shock","power":40}],
  "queries":{
   "typeql":'match $elec isa poketype, has name "Electric"; move-type (typed: $m, category: $elec); $m has name $name, has power $power; fetch { "move": $name, "power": $power };',
   "sql":"SELECT m.name AS move, m.power FROM move m JOIN poketype t ON m.poketype_id = t.id WHERE t.name = 'Electric';",
   "cypher":"MATCH (m:Move)-[:OF_TYPE]->(:PokeType {name: 'Electric'}) RETURN m.name AS move, m.power AS power"}},

 {"question":"List the names of the three highest-power moves, from highest to lowest.","difficulty":"medium","ordered":True,
  "expected":["Hyper Beam","Fire Blast","Hydro Pump"],
  "queries":{
   "typeql":'match $m isa move, has name $name, has power $power; sort $power desc; limit 3; select $name;',
   "sql":"SELECT name FROM move ORDER BY power DESC LIMIT 3;",
   "cypher":"MATCH (m:Move) RETURN m.name ORDER BY m.power DESC LIMIT 3"}},

 {"question":"Which Pokémon can learn an Electric-type move?","difficulty":"hard","expected":["Jolteon","Pikachu","Raichu"],
  "queries":{
   "typeql":'match $elec isa poketype, has name "Electric"; move-type (typed: $m, category: $elec); learning (learner: $p, learned: $m); $p has name $n; select $n; distinct;',
   "sql":"SELECT DISTINCT p.name FROM pokemon p JOIN pokemon_move pm ON pm.pokemon_id = p.id JOIN move m ON pm.move_id = m.id JOIN poketype t ON m.poketype_id = t.id WHERE t.name = 'Electric';",
   "cypher":"MATCH (p:Pokemon)-[:LEARNS]->(:Learning)-[:OF_MOVE]->(m:Move)-[:OF_TYPE]->(:PokeType {name: 'Electric'}) RETURN DISTINCT p.name"}},

 {"question":"Which Pokémon learn Thunderbolt by level-up, and at what level?","difficulty":"hard",
  "expected":[{"pokemon":"Pikachu","level":36},{"pokemon":"Jolteon","level":1}],
  "queries":{
   "typeql":'match $tb isa move, has name "Thunderbolt"; $lvlup isa move-method, has name "Level up"; $l isa learning, links (learner: $p, learned: $tb, via: $lvlup), has level $level; $p has name $pokemon; fetch { "pokemon": $pokemon, "level": $level };',
   "sql":"SELECT p.name AS pokemon, pm.level FROM pokemon_move pm JOIN pokemon p ON pm.pokemon_id = p.id JOIN move m ON pm.move_id = m.id JOIN move_method mm ON pm.method_id = mm.id WHERE m.name = 'Thunderbolt' AND mm.name = 'Level up';",
   "cypher":"MATCH (p:Pokemon)-[:LEARNS]->(l:Learning)-[:OF_MOVE]->(:Move {name: 'Thunderbolt'}), (l)-[:VIA]->(:MoveMethod {name: 'Level up'}) RETURN p.name AS pokemon, l.level AS level"}},

 {"question":"Which types is Water super-effective against?","difficulty":"hard","expected":["Fire","Ground","Rock"],
  "queries":{
   "typeql":'match $water isa poketype, has name "Water"; $e isa efficacy, links (attacker: $water, defender: $def), has factor $f; $f > 100; $def has name $n; select $n;',
   "sql":"SELECT def.name FROM efficacy e JOIN poketype atk ON e.damage_poketype_id = atk.id JOIN poketype def ON e.target_poketype_id = def.id WHERE atk.name = 'Water' AND e.factor > 100;",
   "cypher":"MATCH (:PokeType {name: 'Water'})-[e:EFFECTIVE_AGAINST]->(def:PokeType) WHERE e.factor > 100 RETURN def.name"}},

 {"question":"Which Pokémon have the Static ability?","difficulty":"medium","expected":["Pikachu","Raichu"],
  "queries":{
   "typeql":'match $static isa ability, has name "Static"; ability-slot (owner: $p, granted: $static); $p has name $n; select $n;',
   "sql":"SELECT p.name FROM pokemon p JOIN pokemon_ability pa ON pa.pokemon_id = p.id JOIN ability a ON pa.ability_id = a.id WHERE a.name = 'Static';",
   "cypher":"MATCH (p:Pokemon)-[:HAS_ABILITY]->(:Ability {name: 'Static'}) RETURN p.name"}},

 {"question":"Which Fire-type Pokémon cannot learn any Water-type move?","difficulty":"hard","expected":["Charmander","Flareon"],
  "queries":{
   "typeql":'match $fire isa poketype, has name "Fire"; typing (bearer: $p, category: $fire); not { $water isa poketype, has name "Water"; move-type (typed: $m, category: $water); learning (learner: $p, learned: $m); }; $p has name $n; select $n;',
   "sql":"SELECT p.name FROM pokemon p JOIN pokemon_poketype pt ON pt.pokemon_id = p.id JOIN poketype t ON pt.poketype_id = t.id WHERE t.name = 'Fire' AND NOT EXISTS (SELECT 1 FROM pokemon_move pm JOIN move m ON pm.move_id = m.id JOIN poketype wt ON m.poketype_id = wt.id WHERE pm.pokemon_id = p.id AND wt.name = 'Water');",
   "cypher":"MATCH (p:Pokemon)-[:HAS_TYPE]->(:PokeType {name: 'Fire'}) WHERE NOT EXISTS { MATCH (p)-[:LEARNS]->(:Learning)-[:OF_MOVE]->(:Move)-[:OF_TYPE]->(:PokeType {name: 'Water'}) } RETURN p.name"}},

 # Deliberately unanswerable: no expected value or queries; the only correct
 # response is the UNANSWERABLE token.
 {"question":"What pokemon were on Wolfe Glick's 2016 World Championships winning team?","difficulty":"easy","unanswerable":True},
]

open(f"{ROOT}/data/pokedex/questions.json","w").write(json.dumps({"questions":questions}, indent=2, ensure_ascii=False) + "\n")
print(f"wrote {len(questions)} questions")

# ---- src/pokedex-reference.yml ---------------------------------------------
qs = json.load(open(f"{ROOT}/data/pokedex/questions.json"))["questions"]

# DB order in src/pokedex.yml -> language key per DB.
order = [("typedb","typeql"), ("sql","sql"), ("neo4j","cypher")]

# Must match REPETITIONS in src/runner/src/lib.rs: the runner asks the dummy
# for one response per repetition, so each question's response is scripted this
# many times, consecutively, to stay aligned.
REPETITIONS = 3

def block(query):
    lines = ["        - |", "          ```"]
    for ln in query.split("\n"):
        lines.append("          " + ln if ln else "          ")
    lines.append("          ```")
    return "\n".join(lines)

def token_block(tok):
    # A bare token on its own line (NOT fenced), so the runner reads it as the
    # decline marker rather than as a query to execute.
    return f"        - |\n          {tok}"

responses = []
for _db, lang in order:
    for q in qs:
        resp = token_block("UNANSWERABLE") if q.get("unanswerable") else block(q["queries"][lang])
        responses.extend([resp] * REPETITIONS)

header = """# Reference-equivalence check for the pokedex pilot: the three real DB packages
# each run their own reference query (fed by a dummy model), and the result is
# compared to `expected`. Because all three DBs compare against the SAME
# expected value, an all-accurate run proves the reference queries are mutually
# equivalent as well as correct. This validates the dataset WITHOUT any LLM.
# The unanswerable question is scripted with the UNANSWERABLE token, so it too
# is validated end-to-end.
#
# Prereq: pilot stack up and seeded (cd databases/pokedex && docker compose up -d --build).
# Run:    cargo run -p bench-cli -- src/pokedex-reference.yml results-pokedex-reference.json
#         (expect all records accurate: 13 questions x 3 DBs x 3 repetitions = 117)
#
# Responses are consumed in DB order (typedb, sql, neo4j); each question's
# response is repeated once per repetition. Generated from data/pokedex/questions.json.
dbs:
  - typedb:
      prompts: data/pokedex/typedb
      url: http://localhost:1729
      database: bench
      auth:
        username: admin
        password: password
        tls: false
      schema: data/pokedex/typedb/schema.tql
  - sql:
      prompts: data/pokedex/sql
      url: postgres://bench_ro:bench_ro@localhost/bench
      schema: data/pokedex/sql/schema.sql
  - neo4j:
      prompts: data/pokedex/neo4j
      url: bolt://localhost:7687
      auth:
        username: neo4j
        password: password
      schema: data/pokedex/neo4j/schema.txt
models:
  - dummy:
      responses:
"""

# Retries off: reference queries must be correct first try, and retrying a
# scripted dummy just misaligns the response stream on any failure.
footer = """questionsPath: data/pokedex/questions.json
exampleCounts:
  - 3
maxRetryCounts:
  - 0
"""

open(f"{ROOT}/src/pokedex-reference.yml","w").write(header + "\n".join(responses) + "\n" + footer)
print(f"wrote {len(responses)} scripted reference responses")
