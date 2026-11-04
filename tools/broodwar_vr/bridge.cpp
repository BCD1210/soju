// Experimental local Brood War commander bridge. OpenBW runs the simulation;
// this program does not attach to the Blizzard client or online matchmaking.
#include <cstdint>
#include <BWAPI.h>
#include "BWAPI/GameImpl.h"
#include "BW/BWData.h"
#include <nlohmann/json.hpp>
#include <unistd.h>
#include <fcntl.h>
#include <chrono>
#include <thread>
#include <iostream>
#include <cmath>
#include <limits>
#include <string>
using json = nlohmann::json;

static void emit(const json& j) { std::cout << j.dump() << std::endl; }

class Commander final : public BWAPI::AIModule {
    int commander = -1;
    BWAPI::Unit heroUnit = nullptr;
    std::string input;
    bool eof = false;
    BWAPI::Unit owned(int id) {
        auto u = BWAPI::Broodwar->getUnit(id);
        if (!u || !u->exists() || u->getPlayer() != BWAPI::Broodwar->self())
            throw std::runtime_error("unit is missing or not yours");
        return u;
    }
    BWAPI::Position point(const json& j) {
        double x=j.at("x").get<double>(), y=j.at("y").get<double>();
        if (!std::isfinite(x)||!std::isfinite(y)||x<0||y<0
            ||x>=BWAPI::Broodwar->mapWidth()*32||y>=BWAPI::Broodwar->mapHeight()*32)
            throw std::runtime_error("point outside map");
        return {int(x),int(y)};
    }
    void command(const json& j) {
        json result={{"kind","result"},{"id",j.value("id",0)},{"ok",false}};
        try {
            auto op=j.at("op").get<std::string>();
            if(op=="quit") { BWAPI::Broodwar->leaveGame(); result["ok"]=true; emit(result); return; }
            auto u=owned(j.at("unit").get<int>());
            bool ok=false;
            if(op=="move") ok=u->move(point(j));
            else if(op=="stop") ok=u->stop();
            else if(op=="gather"||op=="follow"||op=="attack") {
                auto target=BWAPI::Broodwar->getUnit(j.at("target").get<int>());
                if(!target||!target->exists()||!target->isVisible()) throw std::runtime_error("target is not visible");
                if(op=="gather") ok=u->gather(target);
                else if(op=="follow") {
                    if(target->getPlayer()!=BWAPI::Broodwar->self()) throw std::runtime_error("follow requires a friendly unit");
                    ok=u->follow(target);
                } else ok=u->attack(target);
            } else if(op=="build") {
                int type=j.at("type").get<int>();
                if(type!=BWAPI::UnitTypes::Terran_Supply_Depot.getID()&&type!=BWAPI::UnitTypes::Terran_Barracks.getID())
                    throw std::runtime_error("unsupported building");
                ok=u->build(BWAPI::UnitType(type),BWAPI::TilePosition(point(j)));
            } else if(op=="train") {
                int type=j.at("type").get<int>();
                if(type!=BWAPI::UnitTypes::Terran_Marine.getID()&&type!=BWAPI::UnitTypes::Terran_SCV.getID())
                    throw std::runtime_error("unsupported unit type");
                ok=u->train(BWAPI::UnitType(type));
            } else throw std::runtime_error("unknown command");
            result["ok"]=ok; result["error"]=ok?"":BWAPI::Broodwar->getLastError().toString();
            result["frame"]=BWAPI::Broodwar->getFrameCount();
        } catch(const std::exception& e) { result["error"]=e.what(); }
        emit(result);
    }
    void readCommands() {
        char bytes[4096];
        for(int batch=0;batch<16;++batch) {
            auto n=read(STDIN_FILENO,bytes,sizeof(bytes));
            if(n==0) {eof=true;break;}
            if(n<0) break;
            input.append(bytes,size_t(n));
            if(input.size()>65536) throw std::runtime_error("command input too large");
        }
        for(int batch=0;batch<64;++batch) {
            auto end=input.find('\n'); if(end==std::string::npos) break;
            std::string line=input.substr(0,end);input.erase(0,end+1);
            try { command(json::parse(line)); }
            catch(const std::exception& e) { emit({{"kind","result"},{"id",0},{"ok",false},{"error",e.what()}}); }
        }
        if(eof) BWAPI::Broodwar->leaveGame();
    }
public:
    void onStart() override {
        auto& g=BWAPI::Broodwar;
        g->setLocalSpeed(0); g->setGUI(false);
        auto start=BWAPI::Position(g->self()->getStartLocation());
        // Explicit commander sandbox addition: one Marine, normal health/movement.
        // Economy is unchanged; no resources or buildings are granted.
        auto hero=g->createUnit(g->self(),BWAPI::UnitTypes::Terran_Marine,start+BWAPI::Position(160,160));
        if(!hero) throw std::runtime_error("unable to place commander");
        heroUnit=hero; commander=hero->getID();
        json heights=json::array();
        for(int y=0;y<g->mapHeight();++y) for(int x=0;x<g->mapWidth();++x) heights.push_back(g->getGroundHeight(x,y));
        emit({{"kind","map"},{"width",g->mapWidth()},{"height",g->mapHeight()},{"terrain",heights},
              {"commander",commander},{"player",g->self()->getID()},
              {"backend","OpenBW"},{"scenario","Terran commander sandbox; idle opponent"}});
    }
    void onFrame() override {
        if(commander<0 && heroUnit && heroUnit->exists()) {
            commander=heroUnit->getID();
            if(commander>=0) heroUnit=nullptr;
        }
        readCommands();
        auto& g=BWAPI::Broodwar;
        if(g->getFrameCount()%3) return;
        json units=json::array();
        for(auto u:g->getAllUnits()) {
            if(!u->exists()||(!u->isVisible()&&u->getPlayer()!=g->self())) continue;
            auto p=u->getPosition();auto t=u->getType();
            units.push_back({{"id",u->getID()},{"type",t.getID()},{"name",t.toString()},
                {"owner",u->getPlayer()->getID()},{"x",p.x},{"y",p.y},{"angle",u->getAngle()},
                {"hp",u->getHitPoints()},{"maxHp",t.maxHitPoints()},{"completed",u->isCompleted()},
                {"resources",u->getResources()},{"order",u->getOrder().toString()},
                {"target",u->getOrderTarget()?u->getOrderTarget()->getID():-1},
                {"building",t.isBuilding()},{"worker",t.isWorker()},{"mineral",t.isMineralField()},
                {"tileWidth",t.tileWidth()},{"tileHeight",t.tileHeight()},
                {"carrying",u->isCarryingMinerals()}});
        }
        auto p=g->self();
        emit({{"kind","state"},{"commander",commander},{"frame",g->getFrameCount()},{"minerals",p->minerals()},{"gas",p->gas()},
              {"supplyUsed",p->supplyUsed()},{"supplyTotal",p->supplyTotal()},{"units",units}});
    }
};

int main(int argc,char** argv) {
    if(argc<2||argc>3||(argc==3&&std::string(argv[2])!="--fast")) {std::cerr<<"usage: soju-bw-bridge MAP.scx [--fast]\n";return 2;}
    const bool fast=argc==3;
    try {
        setenv("OPENBW_ENABLE_UI","0",1);
        fcntl(STDIN_FILENO,F_SETFL,fcntl(STDIN_FILENO,F_GETFL)|O_NONBLOCK);
        BW::GameOwner owner;
        BWAPI::BroodwarImpl_handle h(owner.getGame());
        owner.setPrintTextCallback([](const char* s){std::cerr<<s<<"\n";});
        h->setCharacterName("Soju Commander");
        h->setGameType(BWAPI::GameTypes::Melee);
        h->bwgame.setMapFileName(argv[1]);
        h->createSinglePlayerGame([&]{
            if(!h->self()) {
                for(int i=0;i<BW::PLAYABLE_PLAYER_COUNT;++i) {
                    auto p=h->getPlayer(i);
                    if(p->getType()==BWAPI::PlayerTypes::EitherPreferHuman) {h->switchToPlayer(p);break;}
                }
                return;
            }
            h->self()->setRace(BWAPI::Races::Terran);
            int enemies=0;
            for(int i=0;i<BW::PLAYABLE_PLAYER_COUNT;++i) {
                auto p=h->getPlayer(i);
                if(p==h->self()) continue;
                if(p->getType()==BWAPI::PlayerTypes::EitherPreferHuman||p->getType()==BWAPI::PlayerTypes::EitherPreferComputer) {
                    if(enemies++==0) p->setRace(BWAPI::Races::Zerg); else p->closeSlot();
                }
            }
            h->setRandomSeed(42); h->startGame();
        });
        h->setAIModule(new Commander());
        auto next=std::chrono::steady_clock::now();
        while(!h->bwgame.gameOver()) {
            h->update(); h->bwgame.nextFrame();
            next+=std::chrono::microseconds(42000);
            if(!fast) std::this_thread::sleep_until(next);
        }
        h->update();h->onGameEnd();h->bwgame.leaveGame();
        emit({{"kind","end"}});
    } catch(const std::exception& e) {std::cerr<<"soju-bw: "<<e.what()<<"\n";return 1;}
}
