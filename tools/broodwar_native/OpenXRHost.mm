#include <os/signpost.h>
#include <chrono>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <mach/thread_policy.h>
#include <pthread.h>
// OpenXR frame lifecycle and Metal swapchains. No game simulation runs here.
#define XR_USE_GRAPHICS_API_METAL
#define XR_USE_TIMESPEC
#include <time.h>
#import <QuartzCore/QuartzCore.h>
#import <Metal/Metal.h>
#include <openxr/openxr.h>
#include <openxr/openxr_platform.h>
#include "OpenXRHost.h"
#include <vector>
#include <array>
#include <string>
#include <cstdio>
#include <cstring>
#include <algorithm>

namespace {
void scheduleFrameThread(XrDuration period) {
    if(pthread_main_np())return;
    static thread_local XrDuration configured=0;
    if(period==configured || period<1000000 || period>1000000000)return;
    configured=period;
    mach_timebase_info_data_t base{};
    if(mach_timebase_info(&base)!=KERN_SUCCESS || base.numer==0)return;
    auto ticks=[&](uint64_t ns){return uint32_t(std::min<uint64_t>(UINT32_MAX,ns*base.denom/base.numer));};
    // Bound the frame owner's wake-up delay without changing runtime deadlines.
    // Encoding normally takes less than 1 ms; leave room for control and submission.
    thread_time_constraint_policy_data_t policy{
        ticks(period),ticks(std::min<XrDuration>(period,2000000)),
        ticks(std::min<XrDuration>(period,3000000)),true};
    const auto result=thread_policy_set(pthread_mach_thread_np(pthread_self()),THREAD_TIME_CONSTRAINT_POLICY,
        reinterpret_cast<thread_policy_t>(&policy),THREAD_TIME_CONSTRAINT_POLICY_COUNT);
    fprintf(stderr,"XR frame thread scheduling: period=%.3fms computation=%.3fms constraint=%.3fms result=%d\n",
        double(period)/1e6,double(std::min<XrDuration>(period,2000000))/1e6,
        double(std::min<XrDuration>(period,3000000))/1e6,result);
}
XrInstance instance=XR_NULL_HANDLE;
PFN_xrConvertTimeToTimespecTimeKHR timeToHost=nullptr;
XrSystemId systemId=0;
XrSession session=XR_NULL_HANDLE;
XrSpace localSpace=XR_NULL_HANDLE;
XrActionSet actions=XR_NULL_HANDLE;
XrAction poseAction=XR_NULL_HANDLE,selectAction=XR_NULL_HANDLE,triggerAction=XR_NULL_HANDLE,moveAction=XR_NULL_HANDLE,gripAction=XR_NULL_HANDLE;
XrPath hands[2]={0,0};
XrSpace handSpace[2]={XR_NULL_HANDLE,XR_NULL_HANDLE};
bool previousSelect[2]={false,false},previousGrip[2]={false,false};
id<MTLDevice> device=nil;
id<MTLCommandQueue> queue=nil;
bool running=false,exiting=false,probeOnly=false;
char statusText[256]="OpenXR not started";
uint64_t frames=0;
SojuXREye currentEyes[2]{};
SojuXRFrameTiming frameTiming{};
using TimingClock=std::chrono::steady_clock;
double elapsedMs(TimingClock::time_point start) {
    return std::chrono::duration<double,std::milli>(TimingClock::now()-start).count();
}
struct Eye {
    XrSwapchain chain=XR_NULL_HANDLE;
    uint32_t width=0,height=0;
    std::vector<XrSwapchainImageMetalKHR> images;
};
std::array<Eye,2> eyes;
bool check(XrResult r,const char* what) {
    if(XR_FAILED(r)){std::snprintf(statusText,sizeof(statusText),"%s: OpenXR %d",what,int(r));std::fprintf(stderr,"%s\n",statusText);return false;}
    return true;
}
#define CHECK(x) if(!check((x),#x)) return 0
XrPath path(const char* s) { XrPath p=0;xrStringToPath(instance,s,&p);return p; }
bool action(XrAction& out,const char* name,XrActionType type) {
    XrActionCreateInfo info{XR_TYPE_ACTION_CREATE_INFO};
    info.actionType=type;info.countSubactionPaths=2;info.subactionPaths=hands;
    std::strncpy(info.actionName,name,sizeof(info.actionName)-1);
    std::strncpy(info.localizedActionName,name,sizeof(info.localizedActionName)-1);
    return check(xrCreateAction(actions,&info,&out),"xrCreateAction");
}
bool setupActions() {
    hands[0]=path("/user/hand/left");hands[1]=path("/user/hand/right");
    XrActionSetCreateInfo set{XR_TYPE_ACTION_SET_CREATE_INFO};
    std::strcpy(set.actionSetName,"commander");std::strcpy(set.localizedActionSetName,"Commander");
    if(!check(xrCreateActionSet(instance,&set,&actions),"xrCreateActionSet"))return false;
    if(!action(poseAction,"aim",XR_ACTION_TYPE_POSE_INPUT) ||
       !action(selectAction,"select",XR_ACTION_TYPE_BOOLEAN_INPUT) ||
       !action(triggerAction,"trigger",XR_ACTION_TYPE_FLOAT_INPUT) ||
       !action(moveAction,"move",XR_ACTION_TYPE_VECTOR2F_INPUT) ||
       !action(gripAction,"grip",XR_ACTION_TYPE_FLOAT_INPUT))return false;
    for(const char* profile:{"/interaction_profiles/khr/simple_controller","/interaction_profiles/oculus/touch_controller"}) {
        bool touch=std::strstr(profile,"oculus")!=nullptr;
        std::vector<XrActionSuggestedBinding> bindings;
        for(const char* hand:{"left","right"}) {
            std::string base=std::string("/user/hand/")+hand;
            bindings.push_back({poseAction,path((base+"/input/aim/pose").c_str())});
            bindings.push_back({touch?triggerAction:selectAction,path((base+(touch?"/input/trigger/value":"/input/select/click")).c_str())});
            if(touch){bindings.push_back({moveAction,path((base+"/input/thumbstick").c_str())});
                bindings.push_back({gripAction,path((base+"/input/squeeze/value").c_str())});}
        }
        XrInteractionProfileSuggestedBinding info{XR_TYPE_INTERACTION_PROFILE_SUGGESTED_BINDING};
        info.interactionProfile=path(profile);info.countSuggestedBindings=uint32_t(bindings.size());info.suggestedBindings=bindings.data();
        if(!check(xrSuggestInteractionProfileBindings(instance,&info),"controller bindings"))return false;
    }
    XrSessionActionSetsAttachInfo attach{XR_TYPE_SESSION_ACTION_SETS_ATTACH_INFO};
    attach.countActionSets=1;attach.actionSets=&actions;
    if(!check(xrAttachSessionActionSets(session,&attach),"xrAttachSessionActionSets"))return false;
    for(int h=0;h<2;h++) {
        XrActionSpaceCreateInfo info{XR_TYPE_ACTION_SPACE_CREATE_INFO};
        info.action=poseAction;info.subactionPath=hands[h];info.poseInActionSpace.orientation.w=1;
        if(!check(xrCreateActionSpace(session,&info,&handSpace[h]),"xrCreateActionSpace"))return false;
    }
    return true;
}
void input(XrTime time,void* context,SojuXRControls callback) {
    if(!callback)return;
    XrActiveActionSet set{actions,XR_NULL_PATH};
    XrActionsSyncInfo sync{XR_TYPE_ACTIONS_SYNC_INFO};sync.countActiveActionSets=1;sync.activeActionSets=&set;
    if(XR_FAILED(xrSyncActions(session,&sync)))return;
    SojuXRInput result{};
    for(int h=0;h<2;h++) {
        XrActionStateGetInfo get{XR_TYPE_ACTION_STATE_GET_INFO};get.action=selectAction;get.subactionPath=hands[h];
        XrActionStateBoolean select{XR_TYPE_ACTION_STATE_BOOLEAN};
        xrGetActionStateBoolean(session,&get,&select);
        get.action=triggerAction;XrActionStateFloat trigger{XR_TYPE_ACTION_STATE_FLOAT};
        xrGetActionStateFloat(session,&get,&trigger);
        bool now=(select.isActive && select.currentState) ||
            (trigger.isActive && trigger.currentState>(previousSelect[h]?0.4f:0.6f));
        if(h==0)result.leftPressed=now&&!previousSelect[h];else result.rightPressed=now&&!previousSelect[h];
        if(h==0){result.leftHeld=now;result.leftReleased=!now&&previousSelect[h];}
        else {result.rightHeld=now;result.rightReleased=!now&&previousSelect[h];}
        previousSelect[h]=now;
        get.action=gripAction;XrActionStateFloat grip{XR_TYPE_ACTION_STATE_FLOAT};
        xrGetActionStateFloat(session,&get,&grip);
        bool squeezed=grip.isActive&&grip.currentState>(previousGrip[h]?0.4f:0.6f);
        if(h==0)result.leftGripPressed=squeezed&&!previousGrip[h];else result.rightGripPressed=squeezed&&!previousGrip[h];
        if(h==0)result.leftGripHeld=squeezed;else result.rightGripHeld=squeezed;
        previousGrip[h]=squeezed;
        get.action=moveAction;XrActionStateVector2f move{XR_TYPE_ACTION_STATE_VECTOR2F};
        xrGetActionStateVector2f(session,&get,&move);
        if(move.isActive) {
            if(h==0){result.moveX=move.currentState.x;result.moveY=move.currentState.y;}else result.turnX=move.currentState.x;
        }
    }
    XrSpaceLocation aim{XR_TYPE_SPACE_LOCATION};
    if(XR_SUCCEEDED(xrLocateSpace(handSpace[1],localSpace,time,&aim)) &&
       (aim.locationFlags&XR_SPACE_LOCATION_POSITION_VALID_BIT) &&
       (aim.locationFlags&XR_SPACE_LOCATION_ORIENTATION_VALID_BIT)) {
        result.aimValid=1;
        result.px=aim.pose.position.x;result.py=aim.pose.position.y;result.pz=aim.pose.position.z;
        result.qx=aim.pose.orientation.x;result.qy=aim.pose.orientation.y;result.qz=aim.pose.orientation.z;result.qw=aim.pose.orientation.w;
    }
    XrSpaceLocation left{XR_TYPE_SPACE_LOCATION};
    if(XR_SUCCEEDED(xrLocateSpace(handSpace[0],localSpace,time,&left)) &&
       (left.locationFlags&XR_SPACE_LOCATION_POSITION_VALID_BIT) &&
       (left.locationFlags&XR_SPACE_LOCATION_ORIENTATION_VALID_BIT)) {
        result.leftValid=1;
        result.leftX=left.pose.position.x;result.leftY=left.pose.position.y;result.leftZ=left.pose.position.z;
        result.leftQx=left.pose.orientation.x;result.leftQy=left.pose.orientation.y;result.leftQz=left.pose.orientation.z;result.leftQw=left.pose.orientation.w;
    }
    callback(context,&result);
}
}
extern "C" int soju_xr_init() {
    if(instance!=XR_NULL_HANDLE)return 1;
    std::vector<const char*> extensions{XR_KHR_METAL_ENABLE_EXTENSION_NAME};
    uint32_t extensionCount=0;
    if(XR_SUCCEEDED(xrEnumerateInstanceExtensionProperties(nullptr,0,&extensionCount,nullptr))) {
        std::vector<XrExtensionProperties> supported(extensionCount,{XR_TYPE_EXTENSION_PROPERTIES});
        if(XR_SUCCEEDED(xrEnumerateInstanceExtensionProperties(nullptr,extensionCount,&extensionCount,supported.data()))) {
            for(const auto& extension:supported) if(!std::strcmp(extension.extensionName,XR_KHR_CONVERT_TIMESPEC_TIME_EXTENSION_NAME)) extensions.push_back(XR_KHR_CONVERT_TIMESPEC_TIME_EXTENSION_NAME);
        }
    }
    XrInstanceCreateInfo create{XR_TYPE_INSTANCE_CREATE_INFO};
    std::strcpy(create.applicationInfo.applicationName,"Soju Brood War Commander");
    create.applicationInfo.apiVersion=XR_MAKE_VERSION(1,0,0);
    create.enabledExtensionCount=uint32_t(extensions.size());create.enabledExtensionNames=extensions.data();
    CHECK(xrCreateInstance(&create,&instance));
    if(extensions.size()>1) xrGetInstanceProcAddr(instance,"xrConvertTimeToTimespecTimeKHR",reinterpret_cast<PFN_xrVoidFunction*>(&timeToHost));
    XrSystemGetInfo get{XR_TYPE_SYSTEM_GET_INFO};get.formFactor=XR_FORM_FACTOR_HEAD_MOUNTED_DISPLAY;
    CHECK(xrGetSystem(instance,&get,&systemId));
    PFN_xrGetMetalGraphicsRequirementsKHR requirementsFn=nullptr;
    CHECK(xrGetInstanceProcAddr(instance,"xrGetMetalGraphicsRequirementsKHR",reinterpret_cast<PFN_xrVoidFunction*>(&requirementsFn)));
    XrGraphicsRequirementsMetalKHR requirements{XR_TYPE_GRAPHICS_REQUIREMENTS_METAL_KHR};
    CHECK(requirementsFn(instance,systemId,&requirements));
    device=(__bridge id<MTLDevice>)requirements.metalDevice;
    if(probeOnly){std::strcpy(statusText,"OpenXR loader, system and Metal graphics requirements verified; no session created");return device?1:0;}
    queue=[device newCommandQueue];
    if(!device||!queue){std::strcpy(statusText,"No runtime Metal device");return 0;}
    XrGraphicsBindingMetalKHR binding{XR_TYPE_GRAPHICS_BINDING_METAL_KHR};
    binding.commandQueue=(__bridge void*)queue;
    XrSessionCreateInfo createSession{XR_TYPE_SESSION_CREATE_INFO};
    createSession.next=&binding;createSession.systemId=systemId;
    CHECK(xrCreateSession(instance,&createSession,&session));
    XrReferenceSpaceCreateInfo space{XR_TYPE_REFERENCE_SPACE_CREATE_INFO};
    space.referenceSpaceType=XR_REFERENCE_SPACE_TYPE_LOCAL;space.poseInReferenceSpace.orientation.w=1;
    CHECK(xrCreateReferenceSpace(session,&space,&localSpace));
    if(!setupActions())return 0;
    uint32_t count=0;
    CHECK(xrEnumerateViewConfigurationViews(instance,systemId,XR_VIEW_CONFIGURATION_TYPE_PRIMARY_STEREO,0,&count,nullptr));
    if(count!=2){std::strcpy(statusText,"Stereo view count must be two");return 0;}
    std::array<XrViewConfigurationView,2> views{{{XR_TYPE_VIEW_CONFIGURATION_VIEW},{XR_TYPE_VIEW_CONFIGURATION_VIEW}}};
    CHECK(xrEnumerateViewConfigurationViews(instance,systemId,XR_VIEW_CONFIGURATION_TYPE_PRIMARY_STEREO,2,&count,views.data()));
    uint32_t nformats=0;CHECK(xrEnumerateSwapchainFormats(session,0,&nformats,nullptr));
    std::vector<int64_t> formats(nformats);CHECK(xrEnumerateSwapchainFormats(session,nformats,&nformats,formats.data()));
    int64_t format=MTLPixelFormatBGRA8Unorm_sRGB;
    if(std::find(formats.begin(),formats.end(),format)==formats.end())format=MTLPixelFormatBGRA8Unorm;
    std::fprintf(stderr,"XR swapchain format: %lld (sRGB render view used for UNORM fallback)\n",(long long)format);
    if(std::find(formats.begin(),formats.end(),format)==formats.end()){std::strcpy(statusText,"BGRA8 swapchain unavailable");return 0;}
    for(size_t i=0;i<2;i++) {
        Eye& e=eyes[i];e.width=views[i].recommendedImageRectWidth;e.height=views[i].recommendedImageRectHeight;
        XrSwapchainCreateInfo info{XR_TYPE_SWAPCHAIN_CREATE_INFO};
        info.usageFlags=XR_SWAPCHAIN_USAGE_COLOR_ATTACHMENT_BIT|XR_SWAPCHAIN_USAGE_SAMPLED_BIT;
        // The Swift eye pass renders at 4x MSAA and resolves into this single-sample runtime image.
        info.format=format;info.sampleCount=1;info.width=e.width;info.height=e.height;info.faceCount=1;info.arraySize=1;info.mipCount=1;
        CHECK(xrCreateSwapchain(session,&info,&e.chain));
        uint32_t n=0;CHECK(xrEnumerateSwapchainImages(e.chain,0,&n,nullptr));
        e.images.resize(n,{XR_TYPE_SWAPCHAIN_IMAGE_METAL_KHR});
        CHECK(xrEnumerateSwapchainImages(e.chain,n,&n,reinterpret_cast<XrSwapchainImageBaseHeader*>(e.images.data())));
    }
    std::snprintf(statusText,sizeof(statusText),"OpenXR ready: %ux%u per eye",eyes[0].width,eyes[0].height);
    return 1;
}
extern "C" int soju_xr_probe(){probeOnly=true;int result=soju_xr_init();soju_xr_close();probeOnly=false;return result;}
extern "C" void* soju_xr_device(){return (__bridge void*)device;}
extern "C" const char* soju_xr_status(){return statusText;}
extern "C" int soju_xr_step(void* context,SojuXRDraw draw,SojuXRControls controls) {
    if(!session||exiting)return 0;
    XrEventDataBuffer event{XR_TYPE_EVENT_DATA_BUFFER};
    while(xrPollEvent(instance,&event)==XR_SUCCESS) {
        if(event.type==XR_TYPE_EVENT_DATA_SESSION_STATE_CHANGED) {
            auto state=reinterpret_cast<XrEventDataSessionStateChanged*>(&event)->state;
            if(state==XR_SESSION_STATE_READY&&!running) {
                XrSessionBeginInfo begin{XR_TYPE_SESSION_BEGIN_INFO};begin.primaryViewConfigurationType=XR_VIEW_CONFIGURATION_TYPE_PRIMARY_STEREO;
                CHECK(xrBeginSession(session,&begin));running=true;
            } else if(state==XR_SESSION_STATE_STOPPING&&running) {
                CHECK(xrEndSession(session));running=false;
            } else if(state==XR_SESSION_STATE_EXITING||state==XR_SESSION_STATE_LOSS_PENDING)exiting=true;
        } else if(event.type==XR_TYPE_EVENT_DATA_INSTANCE_LOSS_PENDING)exiting=true;
        event={XR_TYPE_EVENT_DATA_BUFFER};
    }
    if(!running||exiting)return 2;
    XrFrameWaitInfo wait{XR_TYPE_FRAME_WAIT_INFO};XrFrameState state{XR_TYPE_FRAME_STATE};
    static os_log_t frameLog=os_log_create("soju.broodwar","PointsOfInterest");
    os_signpost_interval_begin(frameLog,OS_SIGNPOST_ID_EXCLUSIVE,"Runtime wait");
    frameTiming={};
    const auto waitStart=TimingClock::now();
    XrResult waited=xrWaitFrame(session,&wait,&state);
    const auto waitReturned=TimingClock::now();
    frameTiming.waitMs=elapsedMs(waitStart);
    frameTiming.periodMs=double(state.predictedDisplayPeriod)/1e6;
    if(XR_SUCCEEDED(waited))scheduleFrameThread(state.predictedDisplayPeriod);
    os_signpost_interval_end(frameLog,OS_SIGNPOST_ID_EXCLUSIVE,"Runtime wait");
    CHECK(waited);XrFrameBeginInfo begin{XR_TYPE_FRAME_BEGIN_INFO};CHECK(xrBeginFrame(session,&begin));
    // Balance a begun frame even when image acquisition/render submission fails.
    struct FrameGuard { XrTime time;bool ended=false;~FrameGuard(){if(!ended){XrFrameEndInfo info{XR_TYPE_FRAME_END_INFO};info.displayTime=time;info.environmentBlendMode=XR_ENVIRONMENT_BLEND_MODE_OPAQUE;xrEndFrame(session,&info);}} } frameGuard{state.predictedDisplayTime};
    std::array<XrView,2> views{{{XR_TYPE_VIEW},{XR_TYPE_VIEW}}};
    XrViewLocateInfo locate{XR_TYPE_VIEW_LOCATE_INFO};locate.viewConfigurationType=XR_VIEW_CONFIGURATION_TYPE_PRIMARY_STEREO;
    double predictedHostTime=0;
    if(timeToHost) {
        timespec predicted{},now{};
        if(XR_SUCCEEDED(timeToHost(instance,state.predictedDisplayTime,&predicted)) && clock_gettime(CLOCK_MONOTONIC,&now)==0)
            predictedHostTime=CACurrentMediaTime()+double(predicted.tv_sec-now.tv_sec)+double(predicted.tv_nsec-now.tv_nsec)/1e9;
    }
    locate.displayTime=state.predictedDisplayTime;locate.space=localSpace;
    XrViewState viewState{XR_TYPE_VIEW_STATE};uint32_t count=0;
    XrResult located=xrLocateViews(session,&locate,&viewState,2,&count,views.data());
    std::array<XrCompositionLayerProjectionView,2> projection{{{XR_TYPE_COMPOSITION_LAYER_PROJECTION_VIEW},{XR_TYPE_COMPOSITION_LAYER_PROJECTION_VIEW}}};
    bool valid=state.shouldRender&&XR_SUCCEEDED(located)&&count==2&&
        (viewState.viewStateFlags&XR_VIEW_STATE_ORIENTATION_VALID_BIT)&&(viewState.viewStateFlags&XR_VIEW_STATE_POSITION_VALID_BIT);
    input(state.predictedDisplayTime,context,controls);
    if(valid) {
        for(size_t i=0;i<2;i++) {
            Eye& e=eyes[i];
            const XrView& v=views[i];
            currentEyes[i]=SojuXREye{v.pose.position.x,v.pose.position.y,v.pose.position.z,
                v.pose.orientation.x,v.pose.orientation.y,v.pose.orientation.z,v.pose.orientation.w,
                v.fov.angleLeft,v.fov.angleRight,v.fov.angleUp,v.fov.angleDown,
                (views[0].pose.position.x+views[1].pose.position.x)*0.5f,
                (views[0].pose.position.y+views[1].pose.position.y)*0.5f,
                (views[0].pose.position.z+views[1].pose.position.z)*0.5f,
                e.width,e.height,uint32_t(i),uint32_t((viewState.viewStateFlags&XR_VIEW_STATE_POSITION_TRACKED_BIT)!=0),state.predictedDisplayTime,predictedHostTime};
        }
        for(size_t i=0;i<2;i++) {
            Eye& e=eyes[i];uint32_t index=0;XrSwapchainImageAcquireInfo acquire{XR_TYPE_SWAPCHAIN_IMAGE_ACQUIRE_INFO};
            const auto acquireStart=TimingClock::now();
            CHECK(xrAcquireSwapchainImage(e.chain,&acquire,&index));
            XrSwapchainImageWaitInfo waitImage{XR_TYPE_SWAPCHAIN_IMAGE_WAIT_INFO};waitImage.timeout=XR_INFINITE_DURATION;
            CHECK(xrWaitSwapchainImage(e.chain,&waitImage));
            frameTiming.acquireMs+=elapsedMs(acquireStart);
            const auto renderStart=TimingClock::now();
            draw(context,e.images[index].texture,(__bridge void*)queue,&currentEyes[i]);
            frameTiming.renderMs+=elapsedMs(renderStart);
            XrSwapchainImageReleaseInfo release{XR_TYPE_SWAPCHAIN_IMAGE_RELEASE_INFO};CHECK(xrReleaseSwapchainImage(e.chain,&release));
            projection[i].pose=views[i].pose;projection[i].fov=views[i].fov;
            projection[i].subImage.swapchain=e.chain;projection[i].subImage.imageRect.extent={int32_t(e.width),int32_t(e.height)};
        }
    }
    XrCompositionLayerProjection layer{XR_TYPE_COMPOSITION_LAYER_PROJECTION};layer.space=localSpace;layer.viewCount=2;layer.views=projection.data();
    const XrCompositionLayerBaseHeader* layers[]={reinterpret_cast<const XrCompositionLayerBaseHeader*>(&layer)};
    XrFrameEndInfo end{XR_TYPE_FRAME_END_INFO};end.displayTime=state.predictedDisplayTime;end.environmentBlendMode=XR_ENVIRONMENT_BLEND_MODE_OPAQUE;
    end.layerCount=valid?1:0;end.layers=valid?layers:nullptr;frameGuard.ended=true;
    const auto endStart=TimingClock::now();
    os_signpost_interval_begin(frameLog,OS_SIGNPOST_ID_EXCLUSIVE,"Runtime submit");
    const XrResult ended=xrEndFrame(session,&end);
    os_signpost_interval_end(frameLog,OS_SIGNPOST_ID_EXCLUSIVE,"Runtime submit");
    frameTiming.endMs=elapsedMs(endStart);frameTiming.afterWaitMs=elapsedMs(waitReturned);
    frameTiming.rendered=valid?1:0;
    CHECK(ended);
    frames++;std::snprintf(statusText,sizeof(statusText),"OpenXR frames %llu · poses %s",static_cast<unsigned long long>(frames),valid?"valid":"unavailable");
    if(frames==1||frames%300==0)std::fprintf(stderr,"%s\n",statusText);
    return 1;
}
extern "C" int soju_xr_frame_timing(SojuXRFrameTiming* output) {
    if(!output)return 0;
    *output=frameTiming;return frameTiming.rendered;
}
extern "C" void soju_xr_close() {
    for(auto& e:eyes)if(e.chain){xrDestroySwapchain(e.chain);e.chain=XR_NULL_HANDLE;}
    for(auto& s:handSpace)if(s){xrDestroySpace(s);s=XR_NULL_HANDLE;}
    if(localSpace){xrDestroySpace(localSpace);localSpace=XR_NULL_HANDLE;}
    if(session){xrDestroySession(session);session=XR_NULL_HANDLE;}
    if(actions){xrDestroyActionSet(actions);actions=XR_NULL_HANDLE;}
    if(instance){xrDestroyInstance(instance);instance=XR_NULL_HANDLE;}
    queue=nil;device=nil;running=false;exiting=false;frames=0;
    previousSelect[0]=previousSelect[1]=previousGrip[0]=previousGrip[1]=false;
}

extern "C" int soju_xr_eye(uint32_t index,SojuXREye* packet) {
    if(index>=2||!packet)return 0;*packet=currentEyes[index];return 1;
}
