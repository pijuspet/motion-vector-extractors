#pragma once

#include <stdio.h>
#include <string>
#include <fstream>
extern "C" {
    #include <libavutil/motion_vector.h>    
    #include <libavformat/avformat.h>
    #include <libavcodec/avcodec.h>
}


class MotionVectorWriter {
public:    
    ~MotionVectorWriter() {
        Close();
    }
    bool Open (std::string const& filename);
    int Write(int frame_num, const AVMotionVector *mv, int method_id, size_t size);
    void Close();
private:    
    std::ofstream file;
    int frame_num = 0; // Current frame number   
};



